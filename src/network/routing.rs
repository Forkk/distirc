//! The system which routes IRC messages to the proper places.
//!
//! This is surprisingly complicated :(

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
    PING(Vec<String>, Option<String>),

    // The string is our nick.
    RPL_MYINFO(String),

    /// Represents an unknown response code.
    ///
    /// Since, by definition, we don't know what to do with these, we just send
    /// them off to the network object as is.
    UnknownCode(Response, Vec<String>, Option<String>),
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
                let bc = BufferCmd::PRIVMSG(user.clone(), message);
                route_target(dest, user, cur_nick, bc)
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
                let bc = BufferCmd::NOTICE(sender.clone(), message);

                match sender {
                    Sender::User(u) => route_target(dest, u, cur_nick, bc),
                    Sender::Server(_) => Some(RoutedMsg::NetBuffer(bc)),
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
            let cmd = NetworkCmd::PING(msg.args, msg.body);
            Some(RoutedMsg::Network(cmd))
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
