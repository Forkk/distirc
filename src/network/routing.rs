//! The system which routes IRC messages to the proper places.
//!
//! This is surprisingly complicated :(

use std::fmt;
use std::str::FromStr;
use std::ascii::AsciiExt;
use rotor_irc::{Message, Command, Response};

use common::line::{Sender, User};
use common::types::Nick;

/// Cleaned up forms of channel-specific IRC commands and response codes.
#[derive(Debug, Clone)]
// We allow this so we can match variant names to their associated IRC message
// names.
#[allow(non_camel_case_types)]
pub enum BufferCmd {
    JOIN(User),
    PART(User, Option<String>),
    KICK { by: User, targ: Nick, reason: Option<String> },

    PRIVMSG(User, String),
    NOTICE(Sender, String),
    /// This type represents a CTCP ACTION message. We distinguish these from
    /// `NOTICE` and `PRIVMSG` because they are handled differently.
    ACTION(User, String),

    RPL_NAMREPLY(String),
    RPL_ENDOFNAMES,

    RPL_MOTD(String),
}


/// Represents IRC commands handled by the network object.
#[derive(Debug, Clone)]
// We allow this so we can match variant names to their associated IRC message
// names.
#[allow(non_camel_case_types)]
pub enum NetworkCmd {
    QUIT(User, Option<String>),
    NICK(User, String),

    // The string is our nick.
    RPL_MYINFO(String),

    /// A CTCP query from the given sender. The second arg is the destination it
    /// was sent to.
    ///
    /// Note: If you're looking for CTCP ACTION related stuff, see
    /// `BufferCmd::ACTION`. ACTIONs are not handled as regular CTCP commands
    /// since they act more like a different message type than a CTCP query.
    CtcpQuery(User, String, CtcpMsg),
    /// A CTCP response.
    CtcpReply(User, String, CtcpMsg),

    /// Represents an unknown response code.
    ///
    /// Since, by definition, we don't know what to do with these, we just send
    /// them off to the network object as is.
    UnknownCode(Response, Vec<String>, Option<String>),
}

/// Represents a CTCP query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtcpMsg {
    pub tag: String,
    pub args: Vec<String>,
}


/// Represents specialized forms of IRC commands targeted to a specific handler
/// like a buffer or the network.
#[derive(Debug, Clone)]
pub enum RoutedMsg {
    Channel(String, BufferCmd),
    Private(User, BufferCmd),
    NetBuffer(BufferCmd),
    Network(NetworkCmd),
}


/// Logs an error and returns `None` if the given message doesn't satisfy
macro_rules! check_args {
    // Specifies that there must be a body
    ( $msg: expr; if has body, then $block: block ) => {
        if $msg.body.is_some() {
            $block
        } else {
            error!("Expected a body for {}. Got: {}", $msg.command, $msg);
            None
        }
    };
    // Specifies that there must be at least $min args and a body
    ( $msg: expr; if argc >= $min: expr, and has body, then $block: block ) => {
        if $msg.args.len() >= $min && $msg.body.is_some() {
            $block
        } else {
            error!("Expected a body and at least {} args for {}. Got: {}", $min, $msg.command, $msg);
            None
        }
    };
    // Specifies that there must be at least $min args
    ( $msg: expr; if argc >= $min: expr, then $block: block ) => {
        if $msg.args.len() >= $min {
            $block
        } else {
            error!("Expected at least {} args for {}. Got: {}", $min, $msg.command, $msg);
            None
        }
    };
    // Specifies that there must be exactly $argc args and a body
    ( $msg: expr; if argc == $argc: expr, and has body, then $block: block ) => {
        if $msg.args.len() == $argc && $msg.body.is_some() {
            $block
        } else {
            error!("Expected a body and exactly {} args for {}. Got: {}", $argc, $msg.command, $msg);
            None
        }
    };
    // Specifies that there must be exactly $argc args
    ( $msg: expr; if argc == $argc: expr, then $block: block ) => {
        if $msg.args.len() == $argc {
            $block
        } else {
            error!("Expected exactly {} args for {}. Got: {}", $argc, $msg.command, $msg);
            None
        }
    };
    // Specifies that there must be exactly $argc args **or** a body.
    ( $msg: expr; if argc == $argc: expr, then $argblock: block else if has body $bodyblock: block ) => {
        if $msg.args.len() == $argc {
            $argblock
        } else if $msg.body.is_some() {
            $bodyblock
        } else {
            error!("Expected a body or exactly {} args for {}. Got: {}", $argc, $msg.command, $msg);
            None
        }
    };
}


/// Logs an error and returns if the given sender isn't a user.
///
/// The second argument is used as a message name for logging the error message.
macro_rules! try_user {
    ( $sender: expr, $name: expr ) => {
        match $sender {
            Some(Sender::User(ref user)) => user,
            Some(Sender::Server(_)) => {
                error!("Expected user prefix for {} command, but got a server prefix", $name);
                return None;
            },
            None => {
                error!("Expected user prefix for {} command, but found no prefix", $name);
                return None;
            },
        }
    }
}


/// This function handles routing an IRC message to the proper destinations.
pub fn route_message(msg: Message, cur_nick: &str) -> Option<RoutedMsg> {
    use rotor_irc::Response::*;

    trace!("Routing {:?}", msg);
    let sender = msg.prefix.as_ref().map(|pfx| { Sender::parse_prefix(pfx) });

    match msg.command.clone() {
        Command::JOIN => {
            check_args!(msg; if argc == 1, then {
                let user = try_user!(sender, "JOIN").clone();
                let chan = msg.args[0].clone();
                let bc = BufferCmd::JOIN(user.clone());
                route_target(chan.clone(), user, cur_nick, bc)
            } else if has body {
                let user = try_user!(sender, "JOIN").clone();
                let chan = msg.body.unwrap();
                let bc = BufferCmd::JOIN(user.clone());
                route_target(chan.clone(), user, cur_nick, bc)
            })
        },
        Command::PART => {
            check_args!(msg; if argc == 1, then {
                let user = try_user!(sender, "PART").clone();
                let chan = msg.args[0].clone();
                let bc = BufferCmd::PART(user.clone(), msg.body);
                route_target(chan, user, cur_nick, bc)
            })
        },
        Command::KICK => {
            check_args!(msg; if argc == 2, then {
                let user = try_user!(sender, "KICK").clone();
                let chan = msg.args[0].clone();
                let targ = msg.args[1].clone();
                let bc = BufferCmd::KICK {
                    by: user.clone(),
                    targ: targ,
                    reason: msg.body,
                };
                route_target(chan, user, cur_nick, bc)
            })
        },

        Command::PRIVMSG => {
            check_args!(msg; if argc == 1, and has body, then {
                let user = try_user!(sender, "PRIVMSG").clone();
                let dest = msg.args[0].clone();
                let message = msg.body.unwrap();

                if message.starts_with("\u{1}") {
                    route_ctcp_msg(dest, user, cur_nick, Command::PRIVMSG, message)
                } else {
                    let bc = BufferCmd::PRIVMSG(user.clone(), message);
                    route_target(dest, user, cur_nick, bc)
                }
            })
        },
        Command::NOTICE => {
            check_args!(msg; if argc == 1, and has body, then {
                let sender = if sender.is_none() {
                    error!("Expected a prefix for NOTICE. Args: {:?}", msg.args);
                    return None;
                } else { sender.unwrap() };

                let dest = msg.args[0].clone();
                let message = msg.body.unwrap();

                if message.starts_with("\u{1}") {
                    if let Sender::User(user) = sender {
                        route_ctcp_msg(dest, user, cur_nick, Command::NOTICE, message)
                    } else {
                        error!("Ignored CTCP reply from a server. This isn't supported");
                        return None;
                    }
                } else {
                    let bc = BufferCmd::NOTICE(sender.clone(), message);

                    match sender {
                        Sender::User(u) => route_target(dest, u, cur_nick, bc),
                        Sender::Server(_) => Some(RoutedMsg::NetBuffer(bc)),
                    }
                }
            })
        },

        Command::QUIT => {
            let user = try_user!(sender, "QUIT").clone();
            // The network has to handle routing QUITs, as their routing depends
            // on which channels the quitting user is in.
            Some(RoutedMsg::Network(NetworkCmd::QUIT(user, msg.body)))
        },
        Command::NICK => {
            check_args!(msg; if argc == 1, then {
                let user = try_user!(sender, "NICK").clone();
                let new = msg.args[0].clone();
                // NICKs have the same situation as QUIT messages.
                Some(RoutedMsg::Network(NetworkCmd::NICK(user, new)))
            })
        }


        Command::Response(RPL_NAMREPLY) => {
            check_args!(msg; if argc == 3, and has body, then {
                // The channel is the third arg. I don't know what the other args
                // are, but they probably don't matter...
                let chan = msg.args[2].clone();
                let body = msg.body.unwrap();
                let bc = BufferCmd::RPL_NAMREPLY(body);
                Some(RoutedMsg::Channel(chan, bc))
            })
        },
        Command::Response(RPL_ENDOFNAMES) => {
            check_args!(msg; if argc >= 1, then {
                // The channel is the third arg.
                let chan = msg.args[1].clone();
                let bc = BufferCmd::RPL_ENDOFNAMES;
                Some(RoutedMsg::Channel(chan, bc))
            })
        },

        Command::Response(RPL_MYINFO) => {
            check_args!(msg; if argc >= 1, then {
                Some(RoutedMsg::Network(NetworkCmd::RPL_MYINFO(msg.args[0].clone())))
            })
        },

        // These are all turned into RPL_MOTD buffer commands, since they're
        // mostly the same and don't really warrant additional buffer commands.
        Command::Response(RPL_MOTDSTART) |
        Command::Response(RPL_ENDOFMOTD) |
        Command::Response(RPL_MOTD) => {
            check_args!(msg; if has body, then {
                Some(RoutedMsg::NetBuffer(BufferCmd::RPL_MOTD(msg.body.unwrap())))
            })
        },

        Command::Response(code) => {
            let cmd = NetworkCmd::UnknownCode(code.clone(), msg.args, msg.body);
            Some(RoutedMsg::Network(cmd))
        },


        Command::PING => {
            error!("PING wasn't handled by the connection state machine");
            None
        },
        _ => {
            error!("Ignoring unrouted message: {}", msg);
            None
        },
    }
}

/// Routes a message to private or channel based on the given target string,
/// sender, and current nick.
fn route_target(targ: String, user: User, cur_nick: &str, msg: BufferCmd) -> Option<RoutedMsg> {
    if cur_nick == targ  {
        // NOTE: We don't do anything about the case where the server sends us a
        // JOIN with our nick as target. This shouldn't happen, but it's not
        // inconceivable. Regardless, we'll continue to ignore it for now.
        trace!("Routed private message from {}", user.nick);
        Some(RoutedMsg::Private(user.clone(), msg))
    } else {
        trace!("Routed channel message to {}", targ);
        Some(RoutedMsg::Channel(targ, msg))
    }
}


/// Routes a CTCP message.
fn route_ctcp_msg(targ: String, user: User, cur_nick: &str, cmd: Command, msg: String) -> Option<RoutedMsg> {
    debug_assert!(msg.starts_with("\u{1}"));
    trace!("Parsing CTCP privmsg: {:?}", msg);
    match msg.parse::<CtcpMsg>() {
        Ok(ref msg) if &msg.tag.to_ascii_uppercase() == "ACTION" => {
            let bc = BufferCmd::ACTION(user.clone(), msg.args.join(" "));
            route_target(targ, user, cur_nick, bc)
        },
        Ok(msg) => {
            match cmd {
                Command::PRIVMSG =>
                    Some(RoutedMsg::Network(NetworkCmd::CtcpQuery(user, targ, msg))),
                Command::NOTICE =>
                    Some(RoutedMsg::Network(NetworkCmd::CtcpQuery(user, targ, msg))),
                _ => unreachable!(),
            }
        },
        Err(e) => {
            error!("Error parsing CTCP message: {}", e);
            None
        }
    }
}

/// Writes a CTCP message without the surrounding \u{1} chars.
impl fmt::Display for CtcpMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "{}", self.tag));
        for arg in self.args.iter() {
            try!(write!(f, " {}", arg));
        }
        Ok(())
    }
}

impl FromStr for CtcpMsg {
    type Err = String;

    fn from_str(s: &str) -> Result<CtcpMsg, String> {
        if !s.starts_with("\u{1}") {
            return Err("Not a valid CTCP message".to_owned());
        }
        let s = &s[1..];
        let end = s.find("\u{1}").unwrap_or(s.len());

        let s = &s[..end];
        let mut arg_iter = s.split(" ");
        let tag = arg_iter.next().ok_or("Missing CTCP tag".to_owned());
        let tag = try!(tag).to_ascii_uppercase();
        let args: Vec<_> = arg_iter.map(|s| s.to_owned()).collect();
        Ok(CtcpMsg {
            tag: tag,
            args: args,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::CtcpMsg;

    // Adapted from rotor_irc::message::tests
    macro_rules! parse_fmt_test {
        ( $parse_name: ident, $fmt_name: ident, $body: block ) => {
            #[test]
            fn $parse_name() {
                let (s, msg) = $body;
                assert_eq!(s.parse::<CtcpMsg>().unwrap(), msg)
            }

            #[test]
            fn $fmt_name() {
                let (s, msg) = $body;
                assert_eq!(&format!("\u{1}{}\u{1}", msg), s);
            }
        }
    }

    parse_fmt_test!(parse_ctcp_action, format_ctcp_action, {
        let s = "\u{1}ACTION slaps user with a large trout\u{1}";
        let msg = CtcpMsg {
            tag: "ACTION".to_owned(),
            args: "slaps user with a large trout".split(" ").map(|s| s.to_owned()).collect(),
        };
        (s, msg)
    });

    parse_fmt_test!(parse_ctcp_basic, format_ctcp_basic, {
        let s = "\u{1}ACTION slaps user with a large trout\u{1}";
        let msg = CtcpMsg {
            tag: "ACTION".to_owned(),
            args: "slaps user with a large trout".split(" ").map(|s| s.to_owned()).collect(),
        };
        (s, msg)
    });
}
