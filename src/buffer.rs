use std::collections::VecDeque;
use irc::client::prelude::*;

use line::{LineData, MsgKind, User};
use conn::messages::BufInfo;

/// A buffer within a network.
#[derive(Debug, Clone)]
pub struct Buffer {
    name: String,
    line_id: usize,
    topic: String,
    lines: VecDeque<BufferLine>,
    joined: bool, // users: Vec<String>,
}

// Buffer behavior
impl Buffer {
    pub fn new(name: &str) -> Buffer {
        Buffer {
            name: name.to_owned(),
            line_id: 0,
            topic: String::new(),
            lines: VecDeque::new(),
            joined: false,
        }
    }


    pub fn push_line(&mut self, data: LineData) {
        let line = BufferLine {
            id: self.line_id,
            data: data,
        };
        trace!("Buffer {}: Pushing line {:?}", self.name, line);
        self.line_id += 1;
        self.lines.push_front(line);
    }


    pub fn set_topic(&mut self, topic: String) {
        self.topic = topic;
    }


    pub fn user_msg(&mut self, user: &User, msg: &Message, nick: &str) {
        match msg.command {
            Command::JOIN(_, _, _) => {
                if user.nick == nick {
                    info!("Joined channel {}", self.name);
                    self.joined = true;
                }
                self.push_line(LineData::Join { user: user.clone() })
            }
            Command::PART(_, ref reason) => {
                let reason = reason.clone().unwrap_or("No reason given".to_owned());
                if user.nick == nick {
                    info!("Parted channel {}", self.name);
                    self.joined = false;
                }
                self.push_line(LineData::Part {
                    user: user.clone(),
                    reason: reason,
                })
            }
            Command::KICK(_, ref target, ref reason) => {
                let reason = reason.clone().unwrap_or("No reason given".to_owned());
                if target == nick {
                    info!("Kicked from channel {} by {:?}", self.name, user);
                    self.joined = false;
                }
                self.push_line(LineData::Kick {
                    by: user.clone(),
                    user: target.clone(),
                    reason: reason,
                })
            },
            Command::PRIVMSG(_, ref msg) => {
                self.push_line(LineData::Message {
                    kind: MsgKind::PrivMsg,
                    from: user.nick.clone(),
                    msg: msg.clone(),
                })
            },
            Command::NOTICE(_, ref msg) => {
                self.push_line(LineData::Message {
                    kind: MsgKind::Notice,
                    from: user.nick.clone(),
                    msg: msg.clone(),
                })
            },
            _ => {},
        }
    }
}

// Message data
impl Buffer {
    /// Gets `BufInfo` data for this buffer.
    pub fn as_info(&self) -> BufInfo {
        BufInfo {
            name: self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct BufferLine {
    id: usize,
    data: LineData,
}
