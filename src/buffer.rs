use std::collections::VecDeque;
use irc::client::prelude::*;
use time::{Tm, now};

use common::line::{BufferLine, LineData, MsgKind, User};
use common::messages::{BufInfo, BufTarget, CoreBufMsg};

/// A buffer within a network.
#[derive(Debug, Clone)]
pub struct Buffer {
    id: BufTarget,
    line_id: usize,
    topic: String,
    lines: VecDeque<BufferLine>,
    joined: bool, // users: Vec<String>,
}

// Buffer behavior
impl Buffer {
    pub fn new(id: BufTarget) -> Buffer {
        Buffer {
            id: id,
            line_id: 0,
            topic: String::new(),
            lines: VecDeque::new(),
            joined: false,
        }
    }


    /// Gets the buffer's identifier.
    pub fn id(&self) -> &BufTarget {
        &self.id
    }


    /// Pushes a message into the buffer and posts a `NewLines` message to the
    /// given message buffer.
    pub fn push_line<S>(&mut self, data: LineData, send: &mut S)
        where S: FnMut(CoreBufMsg)
    {
        let line = BufferLine {
            id: self.line_id,
            // time: now(),
            data: data,
        };
        trace!("Buffer {}: Pushing line {:?}", self.id.name(), line);
        self.line_id += 1;
        self.lines.push_front(line.clone());

        send(CoreBufMsg::NewLines(vec![line]));
    }


    pub fn set_topic(&mut self, topic: String) {
        self.topic = topic;
    }


    pub fn user_msg<S>(&mut self, user: &User, msg: &Message, nick: &str, send: &mut S)
        where S: FnMut(CoreBufMsg)
    {
        match msg.command {
            Command::JOIN(_, _, _) => {
                if user.nick == nick {
                    info!("Joined channel {}", self.id.name());
                    self.joined = true;
                }
                self.push_line(LineData::Join { user: user.clone() }, send)
            }
            Command::PART(_, ref reason) => {
                let reason = reason.clone().unwrap_or("No reason given".to_owned());
                if user.nick == nick {
                    info!("Parted channel {}", self.id.name());
                    self.joined = false;
                }
                self.push_line(LineData::Part {
                    user: user.clone(),
                    reason: reason,
                }, send)
            }
            Command::KICK(_, ref target, ref reason) => {
                let reason = reason.clone().unwrap_or("No reason given".to_owned());
                if target == nick {
                    info!("Kicked from channel {} by {:?}", self.id.name(), user);
                    self.joined = false;
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
                    msg: msg.clone(),
                }, send)
            }
            Command::NOTICE(_, ref msg) => {
                self.push_line(LineData::Message {
                    kind: MsgKind::Notice,
                    from: user.nick.clone(),
                    msg: msg.clone(),
                }, send)
            }
            _ => {}
        }
    }
}


// Message data
impl Buffer {
    /// Gets `BufInfo` data for this buffer.
    pub fn as_info(&self) -> BufInfo {
        BufInfo { name: self.id.name().to_owned() }
    }
}
