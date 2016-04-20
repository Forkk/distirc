use std::collections::HashSet;
use std::env;
use irc::client::prelude::*;
use time;

use common::line::{BufferLine, LineData, MsgKind, User};
use common::messages::{NetId, BufInfo, BufTarget, CoreBufMsg};

mod log;

use self::log::BufferLog;

/// A buffer within a network.
#[derive(Debug, Clone)]
pub struct Buffer {
    id: BufTarget,
    line_id: usize,
    topic: String,
    /// Messages received since the core started running.
    front: Vec<BufferLine>,
    /// Messages loaded from logs. These have negative indices.
    back: Vec<BufferLine>,
    joined: bool,
    /// Nicks of users in this channel.
    users: HashSet<String>,
    names_ended: bool,
    log: BufferLog,
}

// Buffer behavior
impl Buffer {
    pub fn new(nid: NetId, id: BufTarget) -> Buffer {
        let mut path = env::current_dir().expect("Failed to get cwd");
        path.push("logs");
        path.push(nid);
        path.push(id.name());
        let mut log = BufferLog::new(path);

        Buffer {
            id: id,
            line_id: 0,
            topic: String::new(),
            front: vec![],
            back: log.fetch_lines(),
            joined: false,
            users: HashSet::new(),
            names_ended: true,
            log: log,
        }
    }


    /// Gets the buffer's identifier.
    pub fn id(&self) -> &BufTarget {
        &self.id
    }


    /// True if a user with the given nick is present in the channel.
    pub fn has_user(&self, nick: &str) -> bool {
        self.users.contains(nick)
    }


    pub fn get_line(&mut self, idx: isize) -> Option<&BufferLine> {
        if idx < self.last_idx() {
            self.back.extend(self.log.fetch_lines());
        };
        if idx < 0 {
            self.back.get((-idx) as usize - 1)
        } else { self.front.get(idx as usize) }
    }


    pub fn last_idx(&self) -> isize {
        if self.back.is_empty() {
            // If the back is empty, the first line in the front buffer is the last index.
            0
        } else {
            -(self.back.len() as isize)
        }
    }

    /// Returns the length of the front buffer. This is the index of the most
    /// recently received message + 1.
    pub fn front_len(&self) -> isize {
        self.front.len() as isize
    }

    /// Returns the length of the back buffer. This is the negative of the index
    /// of the oldest message.
    pub fn back_len(&self) -> isize {
        self.back.len() as isize
    }


    /// Pushes a message into the buffer and posts a `NewLines` message to the
    /// given message buffer.
    pub fn push_line<S>(&mut self, data: LineData, send: &mut S)
        where S: FnMut(CoreBufMsg)
    {
        let line = BufferLine::new(time::now(), data);
        trace!("Buffer {}: Pushing line {:?}", self.id.name(), line);
        self.line_id += 1;
        self.front.push(line.clone());
        self.log.write_lines(vec![line.clone()]);

        send(CoreBufMsg::NewLines(vec![line]));
    }


    pub fn set_topic(&mut self, topic: String) {
        self.topic = topic;
    }


    pub fn user_msg<S>(&mut self, user: &User, msg: &Message, my_nick: &str, send: &mut S)
        where S: FnMut(CoreBufMsg)
    {
        match msg.command {
            Command::JOIN(_, _, _) => {
                if user.nick == my_nick {
                    info!("Joined channel {}", self.id.name());
                    self.joined = true;
                } else {
                    debug!("User {} joined channel {}", user.nick, self.id.name());
                    self.users.insert(user.nick.clone());
                    trace!("Users: {:?}", self.users);
                }
                self.push_line(LineData::Join { user: user.clone() }, send)
            }
            Command::PART(_, ref reason) => {
                let reason = reason.clone().unwrap_or("No reason given".to_owned());
                if user.nick == my_nick {
                    info!("Parted channel {}", self.id.name());
                    self.joined = false;
                    self.users.clear();
                } else {
                    debug!("User {} left channel {}", user.nick, self.id.name());
                    self.users.remove(&user.nick);
                    trace!("Users: {:?}", self.users);
                }
                self.push_line(LineData::Part {
                    user: user.clone(),
                    reason: reason,
                }, send)
            }
            Command::KICK(_, ref target, ref reason) => {
                let reason = reason.clone().unwrap_or("No reason given".to_owned());
                if target == my_nick {
                    info!("Kicked from channel {} by {:?}", self.id.name(), user);
                    self.joined = false;
                    self.users.clear();
                } else {
                    debug!("User {} kicked from channel {}", user.nick, self.id.name());
                    self.users.remove(&user.nick);
                    trace!("Users: {:?}", self.users);
                }
                self.push_line(LineData::Kick {
                    by: user.clone(),
                    user: target.clone(),
                    reason: reason,
                }, send)
            }
            Command::PRIVMSG(_, ref msg) => {
                self.push_line(LineData::Message {
                    kind: MsgKind::PrivMsg,
                    from: user.nick.clone(),
                    ping: msg.contains(my_nick),
                    msg: msg.clone(),
                }, send)
            }
            Command::NOTICE(_, ref msg) => {
                self.push_line(LineData::Message {
                    kind: MsgKind::Notice,
                    from: user.nick.clone(),
                    ping: false,
                    msg: msg.clone(),
                }, send)
            }
            _ => {}
        }
    }

    pub fn user_quit<S>(&mut self, user: &User, msg: Option<String>, send: &mut S)
        where S: FnMut(CoreBufMsg)
    {
        debug!("User {} quit buffer {}", user.nick, self.id.name());
        self.users.remove(&user.nick);
        self.push_line(LineData::Quit {
            user: user.clone(),
            msg: msg,
        }, send);
        trace!("Users: {:?}", self.users);
    }


    /// Handles a `RPL_NAMREPLY` message.
    ///
    /// If this is the first since the last time `end_names` was called, the
    /// user list will be cleared first.
    pub fn handle_names(&mut self, body: &str) {
        for name in body.split(' ') {
            let name = if name.starts_with("@") || name.starts_with("+") {
                &name[1..]
            } else {
                name
            };
            self.users.insert(name.to_owned());
        }
        debug!("User list update: {:?}", self.users);
    }

    /// Called when the name list ends.
    pub fn end_names(&mut self) {
        debug!("Final user list: {:?}", self.users);
        self.names_ended = true;
    }
}

// Message data
impl Buffer {
    /// Gets `BufInfo` data for this buffer.
    pub fn as_info(&self) -> BufInfo {
        BufInfo { id: self.id.clone() }
    }
}
