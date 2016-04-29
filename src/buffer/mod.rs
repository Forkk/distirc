use std::collections::HashSet;
use std::env;
use time;
use rotor_irc::Response;

use common::line::{BufferLine, LineData, MsgKind, User};
use common::messages::{NetId, BufInfo, Alert, BufTarget, CoreBufMsg};

use network::BufferCmd;

mod log;

use handle::UpdateHandle;
use self::log::BufferLog;


/// A buffer within a network.
#[derive(Debug, Clone)]
pub struct Buffer {
    id: BufTarget,
    nid: NetId,
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
        path.push(nid.clone());
        path.push(id.name());
        let mut log = BufferLog::new(path);

        let joined = if let BufTarget::Private(_) = id {
            true
        } else {
            false
        };

        Buffer {
            id: id,
            nid: nid,
            line_id: 0,
            topic: String::new(),
            front: vec![],
            back: log.fetch_lines(),
            joined: joined,
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


    /// Pushes a message into the buffer and sends a `NewLines` message to the
    /// given handle.
    pub fn push_line<U>(&mut self, data: LineData, u: &mut U)
        where U : UpdateHandle<CoreBufMsg>
    {
        let line = BufferLine::new(time::now(), data);
        trace!("Buffer {}: Pushing line {:?}", self.id.name(), line);
        self.line_id += 1;
        self.front.push(line.clone());
        self.log.write_lines(vec![line.clone()]);

        u.send_clients(CoreBufMsg::NewLines(vec![line]));
    }

    /// Sets whether we're joined in this buffer or not and sends a status update.
    fn set_joined<U>(&mut self, joined: bool, u: &mut U)
        where U : UpdateHandle<CoreBufMsg>
    {
        self.joined = joined;
        u.send_clients(CoreBufMsg::State {
            joined: joined,
        })
    }
}

// IRC Message Handling
impl Buffer {
    pub fn handle_cmd<U>(&mut self, cmd: BufferCmd, my_nick: &str, u: &mut U)
        where U : UpdateHandle<CoreBufMsg>
    {
        use network::BufferCmd::*;
        match cmd {
            JOIN(user) => {
                if user.nick == my_nick {
                    debug!("Joined channel {}", self.id.name());
                    self.set_joined(true, u);
                } else {
                    debug!("User {} joined channel {}", user, self.id.name());
                    self.users.insert(user.nick.clone());
                    trace!("Users: {:?}", self.users);
                }

                self.push_line(LineData::Join { user: user }, u)
            },
            PART(user, reason) => {
                let reason = reason.unwrap_or("No reason given".to_owned());
                if user.nick == my_nick {
                    debug!("Parted channel {}", self.id.name());
                    self.set_joined(false, u);
                    self.users.clear();
                } else {
                    debug!("User {} left channel {}", user, self.id.name());
                    self.users.remove(&user.nick);
                    trace!("Users: {:?}", self.users);
                }

                self.push_line(LineData::Part {
                    user: user,
                    reason: reason,
                }, u)
            },
            KICK { by, targ, reason } => {
                let reason = reason.unwrap_or("No reason given".to_owned());
                if targ == my_nick {
                    debug!("Kicked from channel {} by {}", self.id.name(), by);
                    self.set_joined(false, u);
                    self.users.clear();
                } else {
                    debug!("User {} kicked from channel {} by {}", targ, self.id.name(), by);
                    self.users.remove(&targ);
                    trace!("Users: {:?}", self.users);
                }

                self.push_line(LineData::Kick {
                    by: by,
                    user: targ,
                    reason: reason,
                }, u)
            },

            PRIVMSG(user, msg) => {
                if let BufTarget::Channel(ref bid) = self.id {
                    // Check if the message pings us.
                    if msg.contains(my_nick) {
                        // Push a ping
                        let msg = format!("Pinged by {} in channel {}", &user.nick, bid);
                        u.post_alert(Alert::ping(self.nid.clone(), bid.clone(), msg));
                    }
                } else if let BufTarget::Private(ref bid) = self.id {
                    // If it's a PM, send an alert regardless of the contents.
                    let msg = format!("New private message from {}", &user.nick);
                    u.post_alert(Alert::privmsg(self.nid.clone(), bid.clone(), msg));
                }

                self.push_line(LineData::Message {
                    kind: MsgKind::PrivMsg,
                    from: user.nick,
                    msg: msg,
                }, u)
            },
            NOTICE(sender, msg) => {
                // NOTE: Should we check notices for pings?
                self.push_line(LineData::Message {
                    kind: MsgKind::Notice,
                    from: sender.name().to_owned(),
                    msg: msg.clone(),
                }, u)
            },
            ACTION(user, msg) => {
                // NOTE: Should we check actions for pings?
                self.push_line(LineData::Message {
                    kind: MsgKind::Action,
                    from: user.nick.to_owned(),
                    msg: msg.clone(),
                }, u)
            },

            RPL_NAMREPLY(body) => {
                if self.names_ended { self.users.clear(); }
                for name in body.split(' ') {
                    let name = if name.starts_with("@") || name.starts_with("+") {
                        &name[1..]
                    } else {
                        name
                    };
                    self.users.insert(name.to_owned());
                }
                trace!("User list update: {:?}", self.users);
            },
            RPL_ENDOFNAMES => {
                trace!("Final user list: {:?}", self.users);
                self.names_ended = true;
            },

            RPL_MOTD(msg) => {
                // NOTE: Should we check notices for pings?
                self.push_line(LineData::Message {
                    kind: MsgKind::Response(Response::RPL_MOTD.to_u16()),
                    from: "motd".to_owned(),
                    msg: msg.clone(),
                }, u)
            },
        }
    }

    /// Handles `user` quitting.
    pub fn handle_quit<U>(&mut self, user: &User, msg: Option<String>, u: &mut U)
        where U : UpdateHandle<CoreBufMsg>
    {
        debug!("User {} quit buffer {}", user.nick, self.id.name());
        self.users.remove(&user.nick);
        self.push_line(LineData::Quit {
            user: user.clone(),
            msg: msg,
        }, u);
        trace!("Users: {:?}", self.users);
    }

    /// Handles `user` changing nick to `new`.
    pub fn handle_nick<U>(&mut self, user: &User, new: String, u: &mut U)
        where U : UpdateHandle<CoreBufMsg>
    {
        debug!("User {} changed nick to {} in {:?}", user, new, &self.id);
        self.users.remove(&user.nick);
        self.users.insert(new.clone());
        self.push_line(LineData::Nick {
            user: user.clone(),
            new: new,
        }, u);
        trace!("Users: {:?}", self.users);
    }
}

// Message data
impl Buffer {
    /// Gets `BufInfo` data for this buffer.
    pub fn as_info(&self) -> BufInfo {
        BufInfo { id: self.id.clone(), joined: self.joined }
    }
}
