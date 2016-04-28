use std::ops::{Deref, DerefMut};
use std::collections::HashMap;
use rotor_irc::{Message, Command};

use common::messages::{NetInfo, BufTarget, CoreMsg, CoreNetMsg};
use common::line::{LineData, MsgKind};
use common::types::{NetId, Nick};

use config::NetConfig;
use buffer::Buffer;
use handle::UpdateHandle;

mod routing;

pub use self::routing::{RoutedMsg, BufferCmd, NetworkCmd};
use self::routing::route_message;
use conn::irc::IrcSender;

/// This struct represents an IRC network and its state, including the
/// connection.
pub struct IrcNetwork {
    name: NetId,
    pub cfg: NetConfig,
    nick: String,
    // serv_buf: Buffer,
    bufs: HashMap<BufTarget, Buffer>,
    conn: Option<IrcSender>,
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
        if let Some(buf) = self.bufs.get_mut(targ) {
            Some(BufHandle {
                conn: self.conn.as_mut(),
                buf: buf,
                netid: id,
                nick: nick,
            })
        } else { None }
    }


    /// Joins the given channel.
    pub fn join_chan(&mut self, chan: String) {
        // TODO: Tell the client who requested the join that we joined it.
        self.send(Message {
            prefix: None,
            command: Command::JOIN,
            args: vec![chan],
            body: None,
        });
    }

    /// Asks to change to the given nick.
    pub fn change_nick(&mut self, nick: Nick) {
        self.send(Message {
            prefix: None,
            command: Command::NICK,
            args: vec![nick],
            body: None,
        });
    }

    /// Sends the given IRC message.
    ///
    /// If we're not connected to IRC, logs an error and does nothing.
    fn send(&mut self, msg: Message) {
        if let Some(ref mut sender) = self.conn {
            sender.send(msg);
        } else {
            error!("Tried to send a message while disconnected from IRC. Message: {:?}", msg);
        }
    }
}

// Connection Handling
impl IrcNetwork {
    pub fn new(name: String, cfg: &NetConfig) -> IrcNetwork {
        // TODO: Allow configuring reconnection settings.
        // TODO: Allow configuring encoding.
        IrcNetwork {
            name: name.to_owned(),
            cfg: cfg.clone(),
            nick: String::new(),
            conn: None,
            bufs: HashMap::new(),
        }
    }

    /// Called when we've connected to IRC with an `IrcSender` that can be used
    /// to send messages.
    pub fn connected<U>(&mut self, sender: IrcSender, u: &mut U)
        where U : UpdateHandle<CoreNetMsg>
    {
        if self.conn.is_none() {
            self.conn = Some(sender);
            u.send_clients(CoreNetMsg::Connection(true))
        } else {
            panic!("Two connections for network {}", self.name)
        }
    }

    /// Called when we've disconnected from IRC.
    pub fn disconnected<U>(&mut self, u: &mut U)
        where U : UpdateHandle<CoreNetMsg>
    {
        self.conn = None;
        u.send_clients(CoreNetMsg::Connection(false))
    }

    /// Handles messages from IRC.
    pub fn handle_msg<U>(&mut self, msg: Message, u: &mut U)
        where U : UpdateHandle<CoreNetMsg>
    {
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
            Some(RoutedMsg::NetBuffer(cmd)) => {
                let nick = self.nick.clone();
                let buf = self.get_create_buf(BufTarget::Network, u);
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
                    u.send_clients(CoreNetMsg::NickChanged(new.clone()));
                }
                for (targ, ref mut buf) in self.bufs.iter_mut() {
                    if buf.has_user(&user.nick) {
                        let mut buf_uh = u.wrap(|msg| CoreNetMsg::BufMsg(targ.clone(), msg));
                        buf.handle_nick(&user, new.clone(), &mut buf_uh);
                    }
                }
            },
            PING(args, body) => {
                debug!("Replying to PING");
                self.send(Message {
                    prefix: None,
                    command: Command::PONG,
                    args: args,
                    body: body,
                });
            },
            RPL_MYINFO(nick) => {
                info!("Set initial nick to {}", nick);
                self.nick = nick;
            },
            UnknownCode(code, args, body) => {
                warn!(target: "distirc::network::rplcode",
                      "Unknown reply code {:?} args: {:?} body: {:?}", code, args, body);
            },
        }
    }

    /// Returns the given buffer, creating one if it doesn't exist.
    fn get_create_buf<U>(&mut self, targ: BufTarget, u: &mut U) -> &mut Buffer
        where U : UpdateHandle<CoreNetMsg>
    {
        if !self.bufs.contains_key(&targ) {
            let buf = Buffer::new(self.name.clone(), targ.clone());
            u.send_clients(CoreNetMsg::Buffers(vec![buf.as_info()]));
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
    conn: Option<&'a mut IrcSender>,
}

impl<'a> BufHandle<'a> {
    pub fn netid(&self) -> &NetId {
        &self.netid
    }

    /// Sends a PRIVMSG to this buffer's target.
    pub fn send_privmsg<U>(&mut self, msg: String, u: &mut U)
        where U: UpdateHandle<CoreMsg>
    {
        let targ = self.buf.id().clone();
        let dest = match targ {
            BufTarget::Channel(ref dest) => dest,
            BufTarget::Private(ref dest) => dest,
            BufTarget::Network => {
                warn!("Can't send PRIVMSG to network buffer");
                // FIXME: What should this do?
                return;
            },
        };

        let ircmsg = Message {
            prefix: None,
            command: Command::PRIVMSG,
            args: vec![dest.clone()],
            body: Some(msg.clone()),
        };
        if self.try_send(ircmsg, u) {
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

    pub fn part_chan<U>(&mut self, msg: &Option<String>, u: &mut U)
        where U: UpdateHandle<CoreMsg>
    {
        let dest = match *self.buf.id() {
            BufTarget::Channel(ref dest) => dest.clone(),
            BufTarget::Private(ref dest) => dest.clone(),
            BufTarget::Network => {
                warn!("Ignored part command for net buffer");
                return;
            },
        };

        let ircmsg = Message {
            prefix: None,
            command: Command::PART,
            args: vec![dest.clone()],
            body: msg.clone(),
        };
        self.try_send(ircmsg, u);
    }

    fn try_send<U>(&mut self, msg: Message, u: &mut U) -> bool
        where U: UpdateHandle<CoreMsg>
    {
        if let Some(ref mut conn) = self.conn {
            conn.send(msg);
            true
        } else {
            let emsg = "Failed to send IRC message. Not connected".to_owned();
            u.send_clients(CoreMsg::Status(emsg));
            false
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
