use std::io;
use std::thread;
use std::ops::{Deref, DerefMut};
use std::sync::mpsc::{channel, Receiver};
use std::collections::HashMap;
use irc::client::prelude::*;
use rotor::Notifier;

use common::messages::{NetInfo, BufTarget, CoreMsg, CoreNetMsg};
use common::line::{Sender, User, LineData, MsgKind};
use common::types::NetId;

use config::IrcNetConfig;
use buffer::Buffer;

/// This struct represents an IRC network and its state, including the
/// connection.
pub struct IrcNetwork {
    name: NetId,
    cfg: IrcNetConfig,
    conn: Option<(IrcServer, Receiver<Message>)>,
    // serv_buf: Buffer,
    bufs: HashMap<BufTarget, Buffer>,
}

impl IrcNetwork {
    pub fn new(cfg: &IrcNetConfig) -> IrcNetwork {
        IrcNetwork {
            name: cfg.name.clone(),
            cfg: cfg.clone(),
            conn: None,
            // serv_buf: Buffer::new("server"),
            bufs: HashMap::new(),
        }
    }

    /// Attempts to connect to the IRC network.
    pub fn connect(&mut self, notif: Notifier) -> io::Result<()> {
        info!("Connecting to IRC network");
        let c = try!(IrcServer::from_config(self.cfg.cfg.clone()));
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
        // for chan in self.cfg.cfg.channels() {
        //     info!("Adding initial channel: {}", chan);
        //     self.chans.insert(chan.to_owned(), Buffer::new());
        // }
        info!("Sending identification");
        try!(c.identify());
        self.conn = Some((c, rx));
        Ok(())
    }

    /// Processes messages from the server.
    pub fn update(&mut self, msgs: &mut Vec<CoreMsg>) {
        let mut nms = vec![];
        let mut disconn = false;
        'recv: loop {
            let (m, nick) = if let Some((ref conn, ref mut rx)) = self.conn {
                use std::sync::mpsc::TryRecvError;
                match rx.try_recv() {
                    Ok(m) => (m, conn.current_nickname().to_owned()),
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
            self.handle_msg(m, &nick, &mut nms);
        }

        for msg in nms.into_iter() {
            msgs.push(CoreMsg::NetMsg(self.name.clone(), msg));
        }

        if disconn {
            info!("Disconnecting from server");
            self.conn = None;
        }
    }

    pub fn handle_msg(&mut self, msg: Message, nick: &str, msgs: &mut Vec<CoreNetMsg>) {
        let pfx = msg.prefix.clone().map(|p| Sender::parse_prefix(&p));
        match pfx {
            Some(Sender::User(from)) => self.user_msg(&from, &msg, nick, msgs),
            Some(Sender::Server(_)) => {}
            None => {}
        }
    }

    fn user_msg(&mut self, user: &User, msg: &Message, nick: &str, msgs: &mut Vec<CoreNetMsg>) {
        debug!("Handle message {:?}", msg);
        if let Some(dest) = msg.get_dest() {
            let targ = if dest.starts_with("#") {
                // If dest is a channel, route it to that channel's buffer.
                debug!("Routing message to channel {}", dest);
                Some(BufTarget::Channel(dest.clone()))
            } else if dest == nick {
                // If the message was send directly to us, route it to the
                // sender's direct message buffer.
                debug!("Routing message to PM buffer {}", user.nick);
                Some(BufTarget::Private(user.nick.clone()))
            } else {
                None
            };
            if let Some(targ) = targ {
                let mut buf = if !self.bufs.contains_key(&targ) {
                    let buf = Buffer::new(targ.clone());
                    msgs.push(CoreNetMsg::Buffers(vec![buf.as_info()]));
                    self.bufs.entry(targ.clone()).or_insert(buf)
                } else { self.bufs.get_mut(&targ).unwrap() };
                buf.user_msg(user, msg, nick, &mut |msg| {
                    msgs.push(CoreNetMsg::BufMsg(targ.clone(), msg));
                });
            }
        } else {
            warn!("Got user message with no known destination: {}", msg);
        }
    }

    pub fn get_buf(&self, targ: &BufTarget) -> Option<&Buffer> {
        self.bufs.get(targ)
    }

    pub fn get_buf_mut<'a>(&'a mut self, targ: &BufTarget) -> Option<BufHandle<'a>> {
        let id = self.name.clone();
        let irc = if let Some((ref mut irc, _)) = self.conn {
            Some(irc)
        } else { None };
        self.bufs.get_mut(targ).map(|buf| {
            BufHandle {
                irc: irc,
                buf: buf,
                netid: id,
            }
        })
    }

    pub fn join_chan(&mut self, chan: &str) {
        // TODO: Tell the client who requested the join that we joined it.
        if let Some((ref mut irc, _)) = self.conn {
            irc.send(Command::JOIN(chan.to_owned(), None, None))
                .expect("Failed to send IRC message");
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
    netid: NetId,
    buf: &'a mut Buffer,
    irc: Option<&'a mut IrcServer>,
}

impl<'a> BufHandle<'a> {
    pub fn netid(&self) -> &NetId {
        &self.netid
    }

    /// Sends a PRIVMSG to this buffer's target.
    pub fn send_privmsg<S>(&mut self, msg: String, send: &mut S)
        where S: FnMut(CoreMsg)
    {
        let netid = self.netid.clone();
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
            self.buf.push_line(LineData::Message {
                kind: MsgKind::PrivMsg,
                from: irc.current_nickname().to_owned(),
                msg: msg,
            }, &mut |m| {
                send(CoreMsg::NetMsg(netid.clone(), CoreNetMsg::BufMsg(targ.clone(), m)));
            });
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



/// Extension to `Message` and `Command` for querying a message's destination.
pub trait DestBufferExt {
    /// Returns the channel that should handle this message.
    fn get_dest(&self) -> Option<String>;
}

impl DestBufferExt for Message {
    fn get_dest(&self) -> Option<String> {
        match self.command {
            Command::JOIN(ref chan, _, _) => Some(chan.clone()),
            Command::PART(ref chan, _) => Some(chan.clone()),
            Command::TOPIC(ref chan, _) => Some(chan.clone()),
            Command::KICK(ref chan, _, _) => Some(chan.clone()),
            Command::PRIVMSG(ref chan, _) => Some(chan.clone()),
            Command::NOTICE(ref chan, _) => Some(chan.clone()),
            _ => None,
        }
    }
}
