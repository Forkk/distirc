use std::io;
use std::thread;
use std::ops::{Deref, DerefMut};
use std::sync::mpsc::{channel, Receiver};
use std::collections::HashMap;
use irc::client::prelude::*;
use rotor::Notifier;

use common::messages::{NetInfo, BufTarget, CoreMsg, CoreNetMsg};
use common::line::{LineData, MsgKind};
use common::types::{NetId, Nick};

use config::NetConfig;
use buffer::Buffer;
use handle::UpdateHandle;

mod routing;

pub use self::routing::{RoutedMsg, BufferCmd, NetworkCmd};
use self::routing::route_message;

/// This struct represents an IRC network and its state, including the
/// connection.
pub struct IrcNetwork {
    name: NetId,
    cfg: NetConfig,
    nick: String,
    conn: Option<(IrcServer, Receiver<Message>)>,
    // serv_buf: Buffer,
    bufs: HashMap<BufTarget, Buffer>,
}

// External API
impl IrcNetwork {
    /// Gets a reference to the given buffer if it exists.
    pub fn get_buf(&self, targ: &BufTarget) -> Option<&Buffer> {
        self.bufs.get(targ)
    }

    /// Gets a handle to the given buffer if it exists.
    ///
    /// See `BufHandle`
    pub fn get_buf_mut<'a>(&'a mut self, targ: &BufTarget) -> Option<BufHandle<'a>> {
        let id = self.name.clone();
        let nick = self.nick.clone();
        let irc = if let Some((ref mut irc, _)) = self.conn {
            Some(irc)
        } else { None };
        self.bufs.get_mut(targ).map(|buf| {
            BufHandle {
                irc: irc,
                buf: buf,
                netid: id,
                nick: nick,
            }
        })
    }


    /// Joins the given channel.
    pub fn join_chan(&mut self, chan: String) {
        // TODO: Tell the client who requested the join that we joined it.
        if let Some((ref mut irc, _)) = self.conn {
            irc.send(Command::JOIN(chan, None, None)).expect("Failed to send IRC message");
        }
    }

    /// Asks to change to the given nick.
    pub fn change_nick(&mut self, nick: Nick) {
        if let Some((ref mut irc, _)) = self.conn {
            irc.send(Command::NICK(nick)).expect("Failed to send IRC message");
        }
    }
}

// Connection Handling
impl IrcNetwork {
    pub fn new(name: &str, cfg: &NetConfig) -> IrcNetwork {
        IrcNetwork {
            name: name.to_owned(),
            cfg: cfg.clone(),
            nick: String::new(),
            conn: None,
            // serv_buf: Buffer::new("server"),
            bufs: HashMap::new(),
        }
    }

    /// Attempts to connect to the IRC network.
    pub fn connect(&mut self, notif: Notifier) -> io::Result<()> {
        info!("Connecting to IRC network");
        let c = try!(IrcServer::from_config(self.cfg.irc_config()));
        let (tx, rx) = channel();
        let c2 = c.clone();
        thread::spawn(move || {
            debug!("Receiver thread started");
            for m in c2.iter() {
                trace!("Received message: {:?}", m);
                if let Ok(m) = m {
                    tx.send(m).expect("Failed to send channel message");
                    notif.wakeup().expect("Failed to wake update thread");
                }
            }
        });
        info!("Sending identification");
        try!(c.identify());
        self.nick = c.current_nickname().to_owned();
        self.conn = Some((c, rx));
        Ok(())
    }

    /// Processes messages from the server.
    pub fn update<U>(&mut self, u: &mut U)
        where U : UpdateHandle<CoreMsg>
    {
        let nid = self.name.clone();
        let mut net_uh = u.wrap(|msg| {
            CoreMsg::NetMsg(nid.clone(), msg)
        });
        let mut disconn = false;
        'recv: loop {
            let m = if let Some((_, ref mut rx)) = self.conn {
                use std::sync::mpsc::TryRecvError;
                match rx.try_recv() {
                    Ok(m) => m,
                    Err(TryRecvError::Empty) => break 'recv,
                    Err(TryRecvError::Disconnected) => {
                        warn!("Receiver thread stopped. Disconnecting.");
                        disconn = true;
                        break 'recv;
                    }
                }
            } else {
                break 'recv;
            };
            self.handle_msg(m, &mut net_uh);
        }

        if disconn {
            info!("Disconnecting from server");
            self.conn = None;
        }
    }
}

// Message Handling
impl IrcNetwork {
    /// Handles messages from IRC.
    fn handle_msg<U>(&mut self, msg: Message, u: &mut U)
        where U : UpdateHandle<CoreNetMsg>
    {
        trace!("Handling IRC message {:?}", msg);
        match route_message(msg, &self.nick) {
            Some(RoutedMsg::Network(cmd)) => self.handle_net_cmd(cmd, u),
            Some(RoutedMsg::Channel(chan, cmd)) => {
                let nick = self.nick.clone();
                let buf = self.get_create_buf(BufTarget::Channel(chan), u);
                let id = buf.id().clone();
                let mut buf_uh = u.wrap(|msg| CoreNetMsg::BufMsg(id.clone(), msg));
                buf.handle_cmd(cmd, &nick, &mut buf_uh);
            },
            Some(RoutedMsg::Private(user, cmd)) => {
                let nick = self.nick.clone();
                let buf = self.get_create_buf(BufTarget::Private(user.nick), u);
                let id = buf.id().clone();
                let mut buf_uh = u.wrap(|msg| CoreNetMsg::BufMsg(id.clone(), msg));
                buf.handle_cmd(cmd, &nick, &mut buf_uh);
            },
            None => {},
        }
    }

    /// Handles network-routed messages.
    fn handle_net_cmd<U>(&mut self, cmd: NetworkCmd, u: &mut U)
        where U : UpdateHandle<CoreNetMsg>
    {
        use self::routing::NetworkCmd::*;
        match cmd {
            QUIT(user, reason) => {
                for (targ, ref mut buf) in self.bufs.iter_mut() {
                    if buf.has_user(&user.nick) {
                        let mut buf_uh = u.wrap(|msg| CoreNetMsg::BufMsg(targ.clone(), msg));
                        buf.handle_quit(&user, reason.clone(), &mut buf_uh);
                    }
                }
            },
            NICK(user, new) => {
                if user.nick == self.nick {
                    debug!("Nick changed to {}", new);
                    self.nick = new.clone();
                    u.send_msg(CoreNetMsg::NickChanged(new.clone()));
                }
                for (targ, ref mut buf) in self.bufs.iter_mut() {
                    if buf.has_user(&user.nick) {
                        let mut buf_uh = u.wrap(|msg| CoreNetMsg::BufMsg(targ.clone(), msg));
                        buf.handle_nick(&user, new.clone(), &mut buf_uh);
                    }
                }
            },
            UnknownCode(code, args, body) => {
                if let Some(body) = body {
                    error!("Unknown status code {:?} args: {:?} body: {}", code, args, body);
                } else {
                    error!("Unknown status code {:?} args: {:?}", code, args);
                }
            },
        }
    }

    /// Returns the given buffer, creating one if it doesn't exist.
    fn get_create_buf<U>(&mut self, targ: BufTarget, u: &mut U) -> &mut Buffer
        where U : UpdateHandle<CoreNetMsg>
    {
        if !self.bufs.contains_key(&targ) {
            let buf = Buffer::new(self.name.clone(), targ.clone());
            u.send_msg(CoreNetMsg::Buffers(vec![buf.as_info()]));
            self.bufs.entry(targ.clone()).or_insert(buf)
        } else {
            self.bufs.get_mut(&targ).unwrap()
        }
    }
}

// Message data
impl IrcNetwork {
    /// Gets `NetInfo` data for this buffer.
    pub fn to_info(&self) -> NetInfo {
        let mut bufs = vec![];
        for (_id, buf) in self.bufs.iter() { bufs.push(buf.as_info()); }
        NetInfo {
            name: self.name.clone(),
            nick: self.nick.clone(),
            buffers: bufs,
        }
    }
}


/// A reference to a `Buffer` returned by `IrcNetwork::get_buf_mut`.
///
/// This is a struct which derefs to `Buffer` to provide access to the buffer,
/// and also provides additional functions for sending messages to the IRC
/// server.
pub struct BufHandle<'a> {
    nick: Nick,
    netid: NetId,
    buf: &'a mut Buffer,
    irc: Option<&'a mut IrcServer>,
}

impl<'a> BufHandle<'a> {
    pub fn netid(&self) -> &NetId {
        &self.netid
    }

    /// Sends a PRIVMSG to this buffer's target.
    pub fn send_privmsg<U>(&mut self, msg: String, u: &mut U)
        where U: UpdateHandle<CoreMsg>
    {
        if let Some(ref mut irc) = self.irc {
            let targ = self.buf.id().clone();
            let dest = match targ {
                BufTarget::Channel(ref dest) => dest,
                BufTarget::Private(ref dest) => dest,
                BufTarget::Network => {
                    warn!("Can't send PRIVMSG to network");
                    // FIXME: What should this do?
                    return;
                },
            };
            let ircmsg = Message {
                tags: None,
                prefix: None,
                command: Command::PRIVMSG(dest.clone(), msg.clone()),
            };
            irc.send(ircmsg).expect("Failed to send message to IRC server");

            let nid = self.netid.clone();
            let mut buf_uh = u.wrap(|msg| {
                CoreMsg::NetMsg(nid.clone(), CoreNetMsg::BufMsg(targ.clone(), msg))
            });

            self.buf.push_line(LineData::Message {
                kind: MsgKind::PrivMsg,
                from: self.nick.clone(),
                msg: msg,
            }, &mut buf_uh);
        }
    }

    pub fn part_chan(&mut self, msg: &Option<String>) {
        let dest = match *self.buf.id() {
            BufTarget::Channel(ref dest) => dest,
            BufTarget::Private(ref dest) => dest,
            BufTarget::Network => {
                warn!("Ignored part command for net buffer");
                return;
            },
        };
        // TODO: Tell the client who requested the join that we joined it.
        if let Some(ref mut irc) = self.irc {
            irc.send(Command::PART(dest.clone(), msg.clone()))
                .expect("Failed to send IRC message");
        }
    }
}

impl<'a> Deref for BufHandle<'a> {
    type Target = Buffer;
    fn deref(&self) -> &Self::Target { self.buf }
}
impl<'a> DerefMut for BufHandle<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.buf }
}
