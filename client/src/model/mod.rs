//! This module and its submodules implement the "model" component of the
//! client. That is, the back-end parts that connect to the server.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use common::messages::{
    BufTarget, NetId, BufInfo,
    CoreMsg, CoreBufMsg, CoreNetMsg,
    ClientMsg, ClientNetMsg, ClientBufMsg,
    Alert,
};

use conn::ConnThread;

mod buffer;

pub use self::buffer::{Buffer, BufSender, BufKey};

// pub type BufKey = (Option<NetId>, Option<BufId>);

/// Handles communication with the core.
pub struct CoreModel {
    // TODO: This key should probably be made into an enum.
    // In the keys here, when the `NetId` is `None`, the value is a global
    // buffer. When the `BufId` is none and the `NetId` isn't, the buffer is the
    // network's status buffer.
    pub bufs: HashMap<BufKey, BufEntry>,
    conn: ConnThread,
    status: Option<String>,
    // List of new alerts.
    alerts: Vec<Alert>,
}

/// Type for storing buffers in the model.
pub struct BufEntry {
    buf: Rc<RefCell<Buffer>>,
    sender: Option<BufSender>,
}

impl CoreModel {
    pub fn new(status: Buffer, conn: ConnThread) -> CoreModel {
        let mut bufs = HashMap::new();
        let status = Rc::new(RefCell::new(status));
        bufs.insert(BufKey::Status, BufEntry {
            buf: status.clone(),
            sender: None,
        });

        CoreModel {
            bufs: bufs,
            conn: conn,
            alerts: vec![],
            status: None,
        }
    }


    /// Gets the given buffer if it exists.
    pub fn get(&self, key: &BufKey) -> Option<&Rc<RefCell<Buffer>>> {
        self.bufs.get(key).map(|&BufEntry { ref buf, .. }| buf)
    }

    /// Gets the buffer with the given key. Creates one if it doesn't exist.
    pub fn get_or_create(&mut self, key: BufKey) -> Rc<RefCell<Buffer>> {
        if let Some(buf) = self.get(&key) {
            return buf.clone();
        }
        let (buf, bs) = Buffer::new(key.clone());
        let buf = Rc::new(RefCell::new(buf));
        debug!("Created client buffer {:?}", &key);
        self.bufs.insert(key, BufEntry {
            buf: buf.clone(),
            sender: Some(bs)
        });
        buf
    }

    /// Creates a buffer for the given `NetId` and `BufInfo`.
    fn create_remote_buf(&mut self, nid: NetId, info: BufInfo) {
        let key = BufKey::from_targ(nid, info.id);
        let buf = self.get_or_create(key);
        buf.borrow_mut().set_joined(info.joined);
    }


    /// Sets a status message for the UI to show.
    fn status(&mut self, status: String) {
        self.status = Some(status);
    }

    /// Takes a status message if present.
    pub fn take_status(&mut self) -> Option<String> {
        self.status.take()
    }


    /// Returns a vector of new alerts.
    pub fn take_alerts(&mut self) -> Vec<Alert> {
        use std::mem;
        let mut alerts = vec![];
        mem::swap(&mut alerts, &mut self.alerts);
        alerts
    }


    /// Sends a privmsg to the destination channel.
    pub fn send_privmsg(&mut self, key: &BufKey, msg: String) {
        if self.get(key).map_or(false, |b| b.borrow().joined()) {
            self.send_buf(key, ClientBufMsg::SendMsg(msg));
        } else {
            match key {
                key @ &BufKey::Channel(_, _) => {
                    self.status(format!("Can't send to channel {}: not joined", key));
                },
                key @ &BufKey::Private(_, _) => {
                    self.status(format!("Can't send to user {}: not online", key));
                },
                _ => {
                    self.status(format!("Can't send to channel {}: invalid target", key));
                },
            }
        }
    }

    /// Asks the core to join the given channel
    pub fn send_join(&mut self, netid: String, chan: String) {
        self.send_net(&netid, ClientNetMsg::JoinChan(chan));
    }

    /// Asks the core to part from the given channel
    pub fn send_part(&mut self, netid: String, chan: String, msg: String) {
        self.send_buf(&BufKey::Channel(netid, chan), ClientBufMsg::PartChan(Some(msg)));
    }

    /// Requests more logs from the given buffer.
    pub fn send_log_req(&mut self, key: &BufKey, count: usize) {
        self.send_buf(key, ClientBufMsg::FetchLogs(count));
    }


    /// Sends log requests for buffers that need it.
    pub fn send_log_reqs(&mut self) {
        let mut keys = vec![];
        for (key, ent) in self.bufs.iter() {
            let mut buf = ent.buf.borrow_mut();
            if buf.log_req > 0 {
                keys.push((key.clone(), buf.log_req));
                buf.log_req = 0;
            }
        }
        for (k, count) in keys {
            self.send_log_req(&k, count)
        }
    }

    /// Sends a message to the buffer specified by the given key.
    fn send_buf(&mut self, key: &BufKey, msg: ClientBufMsg) {
        trace!("Sending to buffer {:?} message: {:?}", key, msg);
        match *key {
            BufKey::Status => {
                error!("Attempted to send message to client status buffer");
                return;
            },
            BufKey::Network(ref nid) => {
                self.send_net(nid, ClientNetMsg::BufMsg(BufTarget::Network, msg));
            },
            BufKey::Channel(ref nid, ref bid) => {
                self.send_net(nid, ClientNetMsg::BufMsg(BufTarget::Channel(bid.clone()), msg));
            },
            BufKey::Private(ref nid, ref bid) => {
                self.send_net(nid, ClientNetMsg::BufMsg(BufTarget::Private(bid.clone()), msg));
            },
            BufKey::Global(ref bid) => {
                self.send(ClientMsg::BufMsg(bid.clone(), msg));
            },
        }
    }


    fn send_net(&mut self, net: &NetId, msg: ClientNetMsg) {
        trace!("Sending to network {} message: {:?}", net, msg);
        self.send(ClientMsg::NetMsg(net.clone(), msg));
    }

    /// Sends a client message.
    fn send(&mut self, msg: ClientMsg) {
        self.conn.send(msg)
    }


    /// Handles messages and updates the model's state.
    pub fn update(&mut self) {
        while let Some(msg) = self.conn.recv() {
            self.handle_msg(msg);
        }
        for (_, &mut BufEntry { ref mut buf, .. }) in self.bufs.iter_mut() {
            buf.borrow_mut().update();
        }
        self.send_log_reqs();
    }

    fn handle_msg(&mut self, msg: CoreMsg) {
        match msg {
            CoreMsg::Networks(nets) => {
                info!("Adding networks: {:?}", nets);
                for net in nets {
                    for buf in net.buffers {
                        self.create_remote_buf(net.name.clone(), buf);
                    }
                }
            },
            CoreMsg::GlobalBufs(bufs) => {
                debug!("New global buffers: {:?}", bufs);
                for buf in bufs {
                    self.get_or_create(BufKey::Global(buf.name().to_owned()));
                }
            },
            CoreMsg::NetMsg(nid, nmsg) => self.handle_net_msg(nid, nmsg),
            CoreMsg::BufMsg(bid, bmsg) => self.handle_buf_msg(BufKey::Global(bid), bmsg),
            CoreMsg::Alerts(mut alerts) => self.alerts.append(&mut alerts),
        }
    }

    fn handle_net_msg(&mut self, nid: NetId, msg: CoreNetMsg) {
        match msg {
            CoreNetMsg::State { connected } => {
                if connected {
                    self.status(format!("Core connected to network {}", nid));
                } else {
                    self.status(format!("Core disconnected from network {}", nid));
                }
            },
            CoreNetMsg::Buffers(bufs) => {
                for buf in bufs {
                    self.status(format!("Added buffer {}", BufKey::from_targ(nid.clone(), buf.id.clone())));
                    self.create_remote_buf(nid.clone(), buf);
                }
            },
            CoreNetMsg::BufMsg(targ, bmsg) =>
                self.handle_buf_msg(BufKey::from_targ(nid, targ), bmsg),
            CoreNetMsg::Joined(_) => unimplemented!(),
        }
    }

    fn handle_buf_msg(&mut self, key: BufKey, msg: CoreBufMsg) {
        let (buf, bs) = match self.bufs.get_mut(&key) {
            Some(&mut BufEntry { ref mut buf, sender: Some(ref mut bs)}) => (buf, bs),
            _ => {
                error!("Ignoring message for unknown buffer: {:?}", key);
                return;
            },
        };

        match msg {
            CoreBufMsg::State { joined } => {
                buf.borrow_mut().set_joined(joined);
                if joined {
                    self.status = Some(format!("Joined channel {}", key));
                } else {
                    self.status = Some(format!("Parted channel {}", key));
                }
            },
            CoreBufMsg::NewLines(lines) => {
                for line in lines {
                    trace!("Sending line {:?} to front", line);
                    bs.send_front(line);
                }
            },
            CoreBufMsg::Scrollback(lines) => {
                for line in lines {
                    trace!("Sending line {:?} to back", line);
                    bs.send_back(line);
                }
            },
        }
    }
}
