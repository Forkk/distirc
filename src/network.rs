use std::io;
use std::thread;
use std::sync::mpsc::{channel, Receiver};
use std::collections::HashMap;
use irc::client::prelude::*;

use common::messages::NetInfo;
use common::line::{Sender, User};
use common::types::{NetId, BufId};

use config::IrcNetConfig;
use buffer::Buffer;

/// This struct represents an IRC network and its state, including the
/// connection.
pub struct IrcNetwork {
    name: NetId,
    cfg: IrcNetConfig,
    conn: Option<(IrcServer, Receiver<Message>)>,
    // serv_buf: Buffer,
    chans: HashMap<BufId, Buffer>,
    pms: HashMap<BufId, Buffer>,
}

impl IrcNetwork {
    pub fn new(cfg: &IrcNetConfig) -> IrcNetwork {
        IrcNetwork {
            name: cfg.name.clone(),
            cfg: cfg.clone(),
            conn: None,
            // serv_buf: Buffer::new("server"),
            chans: HashMap::new(),
            pms: HashMap::new(),
        }
    }

    /// Attempts to connect to the IRC network.
    pub fn connect(&mut self) -> io::Result<()> {
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
                }
            }
        });
        info!("Sending identification");
        try!(c.identify());
        self.conn = Some((c, rx));
        Ok(())
    }

    /// Processes messages from the server.
    pub fn update(&mut self) {
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
            self.handle_msg(m, &nick);
        }

        if disconn {
            info!("Disconnecting from server");
            self.conn = None;
        }
    }

    pub fn handle_msg(&mut self, msg: Message, nick: &str) {
        let pfx = msg.prefix.clone().map(|p| Sender::parse_prefix(&p));
        match pfx {
            Some(Sender::User(from)) => self.user_msg(&from, &msg, nick),
            Some(Sender::Server(_)) => {}
            None => {}
        }
    }

    fn user_msg(&mut self, user: &User, msg: &Message, nick: &str) {
        debug!("Handle message {:?}", msg);
        if let Some(dest) = msg.get_dest() {
            if dest.starts_with("#") {
                debug!("Routing message to channel {}", dest);
                // If dest is a channel, route it to that channel's buffer.
                let mut chan = self.chans.entry(dest.clone()).or_insert(Buffer::new(&dest.clone()));
                chan.user_msg(user, msg, nick);
            } else if dest == nick {
                debug!("Routing message to PM buffer {}", user.nick);
                // If the message was send directly to us, route it to the
                // sender's direct message buffer.
                let mut pm = self.pms.entry(user.nick.clone()).or_insert(Buffer::new(&user.nick.clone()));
                pm.user_msg(user, msg, nick);
            }
        } else {
            warn!("Got user message with no known destination: {}", msg);
        }
    }
}


// Message data
impl IrcNetwork {
    /// Gets `NetInfo` data for this buffer.
    pub fn to_info(&self) -> NetInfo {
        let mut bufs = vec![];
        for (_id, buf) in self.chans.iter() { bufs.push(buf.as_info()); }
        for (_id, buf) in self.pms.iter() { bufs.push(buf.as_info()); }
        NetInfo {
            name: self.name.clone(),
            buffers: bufs,
        }
    }
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
