//! This module and its submodules implement the "model" component of the
//! client. That is, the back-end parts that connect to the server.

use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use common::messages::{
    BufTarget, BufId, NetId,
    CoreMsg, CoreBufMsg, CoreNetMsg,
    ClientMsg, ClientNetMsg, ClientBufMsg,
};

use conn::ConnThread;

mod buffer;

pub use self::buffer::{Buffer, BufSender};

pub type BufKey = (Option<NetId>, Option<BufId>);

/// Handles communication with the core.
pub struct CoreModel {
    // TODO: This key should probably be made into an enum.
    // In the keys here, when the `NetId` is `None`, the value is a global
    // buffer. When the `BufId` is none and the `NetId` isn't, the buffer is the
    // network's status buffer.
    bufs: HashMap<BufKey, (Rc<RefCell<Buffer>>, Option<BufSender>)>,
    conn: ConnThread,
}

impl CoreModel {
    pub fn new(status: Buffer, conn: ConnThread) -> CoreModel {
        let mut bufs = HashMap::new();
        let status = Rc::new(RefCell::new(status));
        bufs.insert((None, None), (status.clone(), None));

        CoreModel {
            bufs: bufs,
            conn: conn,
        }
    }


    /// Gets the given buffer if it exists.
    pub fn get(&self, key: &BufKey) -> Option<&Rc<RefCell<Buffer>>> {
        self.bufs.get(key).map(|&(ref buf, _)| buf)
    }

    /// Gets the buffer with the given key. Creates one if it doesn't exist.
    pub fn get_or_create(&mut self, key: BufKey) -> Rc<RefCell<Buffer>> {
        if let Some(buf) = self.get(&key) {
            return buf.clone();
        }
        let (buf, bs) = Buffer::new(key.1.clone().unwrap_or("network".to_owned()));
        let buf = Rc::new(RefCell::new(buf));
        debug!("Created client buffer {:?}", &key);
        self.bufs.insert(key, (buf.clone(), Some(bs)));
        buf
    }


    /// Sends a privmsg to the destination channel.
    pub fn send_privmsg(&mut self, key: &BufKey, msg: String) {
        self.send_buf(key, ClientBufMsg::SendMsg(msg));
    }

    /// Asks the core to join the given channel
    pub fn send_join(&mut self, netid: String, chan: String) {
        self.send_net(&netid, ClientNetMsg::JoinChan(chan));
    }

    /// Asks the core to part from the given channel
    pub fn send_part(&mut self, netid: String, chan: String, msg: String) {
        self.send_buf(&(Some(netid), Some(chan)), ClientBufMsg::PartChan(Some(msg)));
    }

    /// Requests more logs from the given buffer.
    pub fn send_log_req(&mut self, key: &BufKey) {
        self.send_buf(key, ClientBufMsg::FetchLogs(10));
    }


    /// Sends log requests for buffers that need it.
    pub fn send_log_reqs(&mut self) {
        let mut keys = vec![];
        for (key, buf) in self.bufs.iter() {
            let mut buf = buf.0.borrow_mut();
            if buf.log_req {
                buf.log_req = false;
                keys.push(key.clone());
            }
        }
        for k in keys {
            self.send_buf(&k, ClientBufMsg::FetchLogs(10))
        }
    }

    /// Sends a message to the buffer specified by the given key.
    fn send_buf(&mut self, key: &BufKey, msg: ClientBufMsg) {
        trace!("Sending to buffer {:?} message: {:?}", key, msg);
        match key {
            &(None, None) => {
                error!("Attempted to send message to system buffer");
                return;
            },
            &(Some(ref net), None) => {
                self.send_net(net, ClientNetMsg::BufMsg(BufTarget::Network, msg));
            },
            &(Some(ref net), Some(ref buf)) => {
                if buf.starts_with("#") {
                    self.send_net(net, ClientNetMsg::BufMsg(BufTarget::Channel(buf.clone()), msg));
                } else {
                    self.send_net(net, ClientNetMsg::BufMsg(BufTarget::Private(buf.clone()), msg));
                }
            },
            &(None, Some(ref buf)) => {
                self.send(ClientMsg::BufMsg(buf.clone(), msg));
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
        for (_, &mut (ref mut buf, _)) in self.bufs.iter_mut() {
            buf.borrow_mut().update()
        }
        self.send_log_reqs();
    }

    fn handle_msg(&mut self, msg: CoreMsg) {
        match msg {
            CoreMsg::Networks(nets) => {
                info!("Adding networks: {:?}", nets);
                for net in nets {
                    for buf in net.buffers {
                        self.get_or_create((Some(net.name.clone()), Some(buf.name().to_owned())));
                    }
                }
            },
            CoreMsg::GlobalBufs(bufs) => {
                debug!("New global buffers: {:?}", bufs);
                for buf in bufs {
                    let key = (None, Some(buf.name().to_owned()));
                    if !self.bufs.contains_key(&key) {
                        let (buf, bs) = Buffer::new(buf.name().to_owned());
                        let buf = Rc::new(RefCell::new(buf));
                        self.bufs.insert(key, (buf, Some(bs)));
                    }
                }
            },
            CoreMsg::NetMsg(nid, nmsg) => self.handle_net_msg(nid, nmsg),
            CoreMsg::BufMsg(bid, bmsg) => self.handle_buf_msg((None, Some(bid)), bmsg),
        }
    }

    fn handle_net_msg(&mut self, net: NetId, msg: CoreNetMsg) {
        match msg {
            CoreNetMsg::State { connected } => {
                if connected {
                    info!("Core connected to network {}", net);
                } else {
                    info!("Core disconnected from network {}", net);
                }
            },
            CoreNetMsg::Buffers(bufs) => {
                debug!("New buffers for network {}: {:?}", net, bufs);
                for buf in bufs {
                    let key = (Some(net.clone()), Some(buf.name().to_owned()));
                    if !self.bufs.contains_key(&key) {
                        let (buf, bs) = Buffer::new(buf.name().to_owned());
                        let buf = Rc::new(RefCell::new(buf));
                        self.bufs.insert(key, (buf, Some(bs)));
                    }
                }
            },
            CoreNetMsg::BufMsg(BufTarget::Network, bmsg) => self.handle_buf_msg((Some(net), None), bmsg),
            CoreNetMsg::BufMsg(BufTarget::Channel(buf), bmsg) =>
                self.handle_buf_msg((Some(net), Some(buf)), bmsg),
            CoreNetMsg::BufMsg(BufTarget::Private(buf), bmsg) =>
                self.handle_buf_msg((Some(net), Some(buf)), bmsg),
            CoreNetMsg::Joined(_) => unimplemented!(),
        }
    }

    fn handle_buf_msg(&mut self, key: (Option<NetId>, Option<BufId>), msg: CoreBufMsg) {
        let (buf, bs) = match self.bufs.get_mut(&key) {
            Some(&mut (ref mut buf, Some(ref mut bs))) => (buf, bs),
            _ => {
                error!("Ignoring message for unknown buffer: {:?}", key);
                return;
            },
        };

        match msg {
            CoreBufMsg::State { joined } => {
                if joined {
                    info!("Joined channel {}", key.1.unwrap_or("*status*".to_owned()));
                } else {
                    info!("Parted channel {}", key.1.unwrap_or("*status*".to_owned()));
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
        buf.borrow_mut().update();
    }
}
