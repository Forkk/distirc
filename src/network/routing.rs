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
            if let Some(ref chan) = msg.body {
                let user = try_user!(sender, "JOIN").clone();
                let bc = BufferCmd::JOIN(user.clone());
                route_target(chan.clone(), user, cur_nick, bc)
            } else if msg.args.len() == 1 {
                let user = try_user!(sender, "JOIN").clone();
                let chan = msg.args[0].clone();
                let bc = BufferCmd::JOIN(user.clone());
                route_target(chan.clone(), user, cur_nick, bc)
            } else {
                error!("Expected body or 1 arg for JOIN. Got: {}", msg);
                None
            }
        },
        Command::PART => {
            if msg.args.len() == 1 {
                let user = try_user!(sender, "PART").clone();
                let chan = msg.args[0].clone();
                let bc = BufferCmd::PART(user.clone(), msg.body);
                route_target(chan, user, cur_nick, bc)
            } else {
                error!("Expected 1 arg for PART. Got: {}", msg);
                None
            }
        },
        Command::KICK => {
            if msg.args.len() == 2 {
                let user = try_user!(sender, "KICK").clone();
                let chan = msg.args[0].clone();
                let targ = msg.args[1].clone();
                let bc = BufferCmd::KICK {
                    by: user.clone(),
                    targ: targ,
                    reason: msg.body,
                };
                route_target(chan, user, cur_nick, bc)
            } else {
                error!("Expected 2 args for KICK. Got: {}", msg);
                None
            }
        },

        Command::PRIVMSG => {
            if msg.args.len() == 1 && msg.body.is_some() {
                let user = try_user!(sender, "PRIVMSG").clone();
                let dest = msg.args[0].clone();
                let message = msg.body.unwrap();
                let bc = BufferCmd::PRIVMSG(user.clone(), message);
                route_target(dest, user, cur_nick, bc)
            } else {
                error!("Expected 1 arg and body for PRIVMSG. Got: {}", msg);
                None
            }
        },
        Command::NOTICE => {
            if msg.args.len() == 1 && msg.body.is_some() {
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
            } else {
                error!("Expected 1 arg and body for NOTICE. Got: {}", msg);
                None
            }
        },

        Command::QUIT => {
            let user = try_user!(sender, "QUIT").clone();
            // The network has to handle routing QUITs, as their routing depends
            // on which channels the quitting user is in.
            Some(RoutedMsg::Network(NetworkCmd::QUIT(user, msg.body)))
        },
        Command::NICK => {
            if msg.args.len() == 1 {
                let user = try_user!(sender, "NICK").clone();
                let new = msg.args[0].clone();
                // NICKs have the same situation as QUIT messages.
                Some(RoutedMsg::Network(NetworkCmd::NICK(user, new)))
            } else {
                error!("Expected 1 arg for NICK. Got: {}", msg);
                None
            }
        }


        Command::Response(RPL_NAMREPLY) => {
            if msg.args.len() != 3 || msg.body.is_none() {
                error!("Expected 3 args and a body to RPL_NAMREPLY. Got: {}", msg);
                return None;
            }
            // The channel is the third arg. I don't know what the other args
            // are, but they probably don't matter...
            let chan = msg.args[2].clone();
            let body = msg.body.unwrap();
            let bc = BufferCmd::RPL_NAMREPLY(body);
            Some(RoutedMsg::Channel(chan, bc))
        },
        Command::Response(RPL_ENDOFNAMES) => {
            if msg.args.len() >= 1 {
                error!("Expected at least 1 arg to RPL_ENDOFNAMES. Got: {}", msg);
                return None;
            }
            // The channel is the third arg.
            let chan = msg.args[1].clone();
            let bc = BufferCmd::RPL_ENDOFNAMES;
            Some(RoutedMsg::Channel(chan, bc))
        },

        Command::Response(RPL_MYINFO) => {
            if msg.args.len() < 1 {
                error!("Expected at least 1 arg to RPL_MYINFO. Got: {}", msg);
                None
            } else {
                Some(RoutedMsg::Network(NetworkCmd::RPL_MYINFO(msg.args[0].clone())))
            }
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
