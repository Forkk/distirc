use std::fmt;
use std::error::Error;
use std::collections::HashMap;
use std::collections::hash_map;
use rotor::Notifier;
use rotor_irc::{Message, Command};

use common::messages::{NetInfo, BufTarget, CoreMsg, CoreNetMsg, SendMsgKind};
use common::line::{LineData, MsgKind};
use common::types::{NetId, Nick};

use config::NetConfig;
use buffer::Buffer;
use handle::UpdateHandle;

mod routing;
mod sender;

pub use self::routing::{RoutedMsg, BufferCmd, NetworkCmd};
use self::routing::route_message;

use self::sender::IrcSender;
pub use self::sender::IrcSendRx;


/// An IRC network.
///
/// This consists of two main parts, a `BufSet`, which is a container for the
/// network's buffers, and an optional `IrcConn`, which, when the network is
/// connected, keeps track of the state of the IRC connection and provides an
/// interface for sending messages to the IRC netowrk.
pub struct IrcNetwork {
    id: NetId,
    nick: Nick,
    pub cfg: NetConfig,
    bufs: HashMap<BufTarget, Buffer>,
    conn: Option<IrcSender>,
}

/// Buffer access and other info
impl IrcNetwork {
    /// Gets the network ID.
    pub fn id(&self) -> &NetId {
        &self.id
    }

    /// Returns an iterator over the buffers in the set.
    pub fn iter_bufs(&self) -> IterBufs {
        self.bufs.iter()
    }

    /// Returns an iterator over the buffers in the set.
    pub fn iter_bufs_mut(&mut self) -> IterBufsMut {
        self.bufs.iter_mut()
    }

    /// Gets a reference to the given buffer if it exists.
    pub fn get_buf(&self, targ: &BufTarget) -> Option<&Buffer> {
        self.bufs.get(targ)
    }

    /// Gets a mutable reference to the given buffer.
    pub fn get_buf_mut<'a>(&'a mut self, targ: &BufTarget) -> Option<&mut Buffer> {
        self.bufs.get_mut(targ)
    }
}

pub type IterBufs<'a> = hash_map::Iter<'a, BufTarget, Buffer>;
pub type IterBufsMut<'a> = hash_map::IterMut<'a, BufTarget, Buffer>;

/// IRC message handling
impl IrcNetwork {
    pub fn new(id: String, cfg: &NetConfig) -> IrcNetwork {
        // TODO: Allow configuring reconnection settings.
        // TODO: Allow configuring encoding.
        IrcNetwork {
            id: id.to_owned(),
            cfg: cfg.clone(),
            nick: String::new(),
            conn: None,
            bufs: HashMap::new(),
        }
    }

    /// Used to register a connection state machine as the connection for this
    /// network.
    ///
    /// When a connection connects to an IRC network, it calls this method with
    /// a notifier that will wake it up and the current nick. This method will
    /// return an `IrcConnRx` object which the connection should read messages
    /// from when it is woken up by the notifier.
    ///
    /// # Panics
    ///
    /// Currently this panics if there is already a connection. This should
    /// probably change in the future.
    pub fn register_conn<U>(&mut self, notif: Notifier, u: &mut U) -> IrcSendRx
        where U : UpdateHandle<CoreMsg>
    {
        if self.conn.is_none() {
            let (conn, rx) = IrcSender::new(notif);
            self.conn = Some(conn);
            u.send_clients(CoreMsg::NetMsg(self.id.clone(), CoreNetMsg::Connection(true)));
            rx
        } else {
            panic!("Two connections for network {}", self.id)
        }
    }

    /// Unregisters the current connection.
    ///
    /// This will eventually be done automatically if the paired `IrcConnRx` is
    /// dropped.
    pub fn disconnect<U>(&mut self, u: &mut U)
        where U : UpdateHandle<CoreNetMsg>
    {
        self.conn = None;
        u.send_clients(CoreNetMsg::Connection(false));
    }

    /// Handles a message from IRC
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

    /// Handles network-routed IRC messages.
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
            RPL_MYINFO(nick) => {
                info!("Set initial nick to {}", nick);
                self.nick = nick;
            },

            CtcpQuery(ref user, _, ref query) if query.tag == "VERSION" => {
                info!("Received CTCP version request from {}", user.nick);
                {
                    let mut buf_uh = u.wrap(|msg| CoreNetMsg::BufMsg(BufTarget::Network, msg));
                    let mut buf = self.get_buf_mut(&BufTarget::Network).unwrap();
                    buf.push_line(LineData::Message {
                        kind: MsgKind::Status,
                        from: user.nick.clone(),
                        msg: format!("*CTCP VERSION request*"),
                    }, &mut buf_uh);
                }

                let vsn = env!("CARGO_PKG_VERSION");
                let vsn_msg = format!("\u{1}VERSION distirc {}\u{1}", vsn);
                // We don't care too much if we fail to respond to CTCP.
                let _ = self.send(Message {
                    prefix: None,
                    command: Command::NOTICE,
                    args: vec![user.nick.clone()],
                    body: Some(vsn_msg),
                }, u);
            },
            CtcpQuery(_, _, query) => {
                info!("Ignoring unsupported CTCP query {}", query.tag);
            },
            CtcpReply(_, _, query) => {
                info!("Ignoring unsupported CTCP reply {}", query.tag);
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
            let buf = Buffer::new(self.id.clone(), targ.clone());
            u.send_clients(CoreNetMsg::Buffers(vec![buf.as_info()]));
            self.bufs.entry(targ.clone()).or_insert(buf)
        } else {
            self.bufs.get_mut(&targ).unwrap()
        }
    }
}

/// IRC message sending
impl IrcNetwork {
    /// Sends the given IRC message.
    ///
    /// If we're not connected to IRC, logs an error and does nothing.
    pub fn send<U>(&mut self, msg: Message, u: &mut U) -> Result<(), IrcSendErr>
        where U : UpdateHandle<CoreNetMsg>
    {
        Self::send_with_conn(&mut self.conn, msg, u)
    }

    // For sending without borrowing `self` completely
    fn send_with_conn<U>(conn: &mut Option<IrcSender>, msg: Message, u: &mut U) -> Result<(), IrcSendErr>
        where U : UpdateHandle<CoreNetMsg>
    {
        if let Some(sender) = conn.take() {
            if let Some(sender) = sender.send(msg.clone()) {
                info!("Sent message: {}", msg);
                *conn = Some(sender);
                Ok(())
            } else {
                error!("Connection dropped while sending message: {}", msg);
                *conn = None;
                u.send_clients(CoreNetMsg::Connection(false));
                Err(IrcSendErr::Disconnected)
            }
        } else {
            error!("Tried to send a message while disconnected from IRC. Message: {}", msg);
            Err(IrcSendErr::Disconnected)
        }
    }

    /// Attempts to join the given channel.
    pub fn send_join_chan<U>(&mut self, chan: String, u: &mut U)
                             -> Result<(), IrcSendErr>
        where U : UpdateHandle<CoreNetMsg>
    {
        self.send(Message::new(None, Command::JOIN, vec![chan], None), u)
    }

    /// Attempts to join the given channel.
    pub fn send_part_chan<U>(&mut self, chan: String, optmsg: Option<String>, u: &mut U)
                             -> Result<(), IrcSendErr>
        where U : UpdateHandle<CoreNetMsg>
    {
        self.send(Message::new(None, Command::PART, vec![chan], optmsg), u)
    }

    /// Changes nick to the given nick.
    pub fn send_change_nick<U>(&mut self, nick: String, u: &mut U)
                               -> Result<(), IrcSendErr>
        where U : UpdateHandle<CoreNetMsg>
    {
        self.send(Message::new(None, Command::NICK, vec![nick], None), u)
    }

    /// Sends a `PrivMsg`, `Action`, or `Notice` to the buffer specified by
    /// `targ`.
    ///
    /// # Errors
    ///
    /// If no such buffer exists, we aren't joined in the target
    /// channel, or the target user is offline, returns
    /// `Err(IrcSendErr::Unavail)`.
    ///
    /// If we're not connected to IRC, returns `Err(IrcSendErr::Disconnected)`.
    pub fn send_chat_msg<U>(&mut self, targ: BufTarget, msg: String, kind: SendMsgKind, u: &mut U)
                        -> Result<(), IrcSendErr>
        where U : UpdateHandle<CoreNetMsg>
    {
        let buf = try!(self.bufs.get_mut(&targ).ok_or(IrcSendErr::Unavail));
        let dest = match targ {
            BufTarget::Channel(ref dest) => dest.clone(),
            BufTarget::Private(ref dest) => dest.clone(),
            BufTarget::Network => {
                warn!("Can't send PRIVMSG to network buffer");
                // FIXME: What should this do?
                return Err(IrcSendErr::BadTarget);
            },
        };
        let ircmsg = match kind {
            SendMsgKind::PrivMsg =>
                Message::new(None, Command::PRIVMSG, vec![dest.clone()], Some(msg.clone())),
            SendMsgKind::Notice =>
                Message::new(None, Command::NOTICE, vec![dest.clone()], Some(msg.clone())),
            SendMsgKind::Action => Message {
                prefix: None,
                command: Command::PRIVMSG,
                args: vec![dest.clone()],
                body: Some(format!("\u{1}ACTION {}\u{1}", msg)),
            },
        };
        let r = Self::send_with_conn(&mut self.conn, ircmsg, u);
        if r.is_ok() {
            let mut buf_uh = u.wrap(|msg| CoreNetMsg::BufMsg(targ.clone(), msg));

            debug_assert!(!self.nick.is_empty(), "Sending message with empty nick");
            buf.push_line(LineData::Message {
                kind: kind.to_msg_kind(),
                from: self.nick.clone(),
                msg: msg,
            }, &mut buf_uh);
        }
        r
    }
}

/// Message data
impl IrcNetwork {
    /// Gets `NetInfo` data for this buffer.
    pub fn to_info(&self) -> NetInfo {
        let mut bufs = vec![];
        for (_id, buf) in self.bufs.iter() { bufs.push(buf.as_info()); }
        NetInfo {
            id: self.id.clone(),
            nick: self.nick.clone(),
            buffers: bufs,
        }
    }
}


/// Errors that can happen when trying to send messages to IRC.
#[derive(Debug, Clone)]
pub enum IrcSendErr {
    /// The target of the action (i.e., the buffer or user we're sending to) is
    /// not available.
    Unavail,
    /// We're not connected to IRC anymore.
    Disconnected,
    /// The given target is invalid.
    BadTarget,
}

impl Error for IrcSendErr {
    fn description(&self) -> &str {
        match *self {
            IrcSendErr::Unavail => "Target buffer is unavailable",
            IrcSendErr::Disconnected => "Not connected to IRC",
            IrcSendErr::BadTarget => "Invalid target",
        }
    }
}

impl fmt::Display for IrcSendErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}
