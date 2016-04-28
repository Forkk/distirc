// use irc::client::prelude::*;
use std::fmt;
use time;
use time::{Tm, Timespec};
use serde::{Serializer, Deserializer};

use types::Nick;

include!(concat!(env!("OUT_DIR"), "/line.rs"));

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}!{}@{}", self.nick, self.ident, self.host)
    }
}

impl Sender {
    pub fn parse_prefix(pfx: &str) -> Sender {
        if let Some(nick_end) = pfx.find('!') {
            let nick = &pfx[..nick_end];
            if let Some(ident_end) = pfx.find('@') {
                let ident = &pfx[nick_end+1..ident_end];
                let host = &pfx[ident_end+1..];
                return Sender::User(User {
                    nick: nick.to_owned(),
                    ident: ident.to_owned(),
                    host: host.to_owned(),
                })
            }
        }
        Sender::Server(pfx.to_owned())
    }

    /// If this sender is a server, returns its name. If it's a user, returns
    /// the user's nick.
    pub fn name(&self) -> &str {
        match *self {
            Sender::Server(ref name) => name,
            Sender::User(ref u) => &u.nick,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_prefix() {
        assert_eq!(Sender::User(User {
            nick: "Forkk".to_owned(), ident: "~forkk".to_owned(), host: "irc.forkk.net".to_owned(),
        }), Sender::parse_prefix("Forkk!~forkk@irc.forkk.net"));
    }
}
