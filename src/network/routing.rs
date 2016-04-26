//! The system which routes IRC messages to the proper places.
//!
//! This is surprisingly complicated :(

use irc::client::prelude::*;

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
    NOTICE(User, String),

    RPL_NAMREPLY(String),
    RPL_ENDOFNAMES,
}


/// Represents IRC commands handled by the network object.
#[derive(Debug, Clone)]
pub enum NetworkCmd {
    QUIT(User, Option<String>),
    NICK(User, String),

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
    trace!("Routing {:?}", msg);
    // Parse the prefix first, as we'll need it later.
    let sender = msg.prefix.as_ref().map(|p| Sender::parse_prefix(&p));

    match msg.command {
        Command::JOIN(chan, _, _) => {
            let user = try_user!(sender, "JOIN").clone();
            let bc = BufferCmd::JOIN(user.clone());
            route_target(chan, user, cur_nick, bc)
        },
        Command::PART(chan, msg) => {
            let user = try_user!(sender, "PART").clone();
            let bc = BufferCmd::PART(user.clone(), msg.clone());
            route_target(chan, user, cur_nick, bc)
        },
        Command::KICK(chan, targ, msg) => {
            let user = try_user!(sender, "KICK").clone();
            let bc = BufferCmd::KICK {
                by: user.clone(),
                targ: targ.clone(),
                reason: msg.clone(),
            };
            route_target(chan, user, cur_nick, bc)
        },

        Command::PRIVMSG(dest, msg) => {
            let user = try_user!(sender, "PRIVMSG").clone();
            let bc = BufferCmd::PRIVMSG(user.clone(), msg.clone());
            route_target(dest, user, cur_nick, bc)
        },
        Command::NOTICE(dest, msg) => {
            let user = try_user!(sender, "NOTICE").clone();
            let bc = BufferCmd::NOTICE(user.clone(), msg.clone());
            route_target(dest, user, cur_nick, bc)
        },

        Command::QUIT(msg) => {
            let user = try_user!(sender, "QUIT").clone();
            // The network has to handle routing QUITs, as their routing depends
            // on which channels the quitting user is in.
            Some(RoutedMsg::Network(NetworkCmd::QUIT(user, msg.clone())))
        },
        Command::NICK(new) => {
            let user = try_user!(sender, "NICK").clone();
            // NICKs have the same situation as QUIT messages.
            Some(RoutedMsg::Network(NetworkCmd::NICK(user, new)))
        }


        Command::Response(Response::RPL_NAMREPLY, args, Some(body)) => {
            if args.len() < 3 {
                error!("Expected 3 args to RPL_NAMREPLY. Got: {:?}", args);
                return None;
            }
            // The channel is the third arg. I don't know what the other args
            // are...
            let ref chan = args[2];
            let bc = BufferCmd::RPL_NAMREPLY(body);
            Some(RoutedMsg::Channel(chan.clone(), bc))
        },
        Command::Response(Response::RPL_ENDOFNAMES, args, _) => {
            if args.len() < 1 {
                error!("Expected 1 arg to RPL_ENDOFNAMES. Got: {:?}", args);
                return None;
            }
            // The channel is the third arg.
            let ref chan = args[1];
            let bc = BufferCmd::RPL_ENDOFNAMES;
            Some(RoutedMsg::Channel(chan.clone(), bc))
        },

        Command::Response(code, args, body) => {
            let cmd = NetworkCmd::UnknownCode(code, args, body);
            Some(RoutedMsg::Network(cmd))
        },

        // Ignore pings -- the IRC library responds to them for us.
        Command::PING(_, _) => None,
        _ => {
            error!("Ignoring unrouted message: {:?}", msg);
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
