use irc::client::prelude::*;

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub enum LineData {
    Message {
        kind: MsgKind,
        from: String,
        msg: String,
    },
    Topic {
        by: Option<String>,
        topic: String
    },
    Join {
        user: User,
    },
    Part {
        user: User,
        reason: String,
    },
    Kick {
        by: User,
        user: String,
        reason: String,
    },
    Quit {
        user: User,
        msg: String,
    },
}

#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub enum MsgKind {
    PrivMsg,
    Notice,
    // FIXME: The below is not encodable
    // /// IRC response codes
    // Response(Response),
    /// Special status messages
    Status,
}

/// Sender of a message
#[derive(Debug, Clone, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub enum Sender {
    User(User),
    Server(String),
}

/// An IRC user sender
#[derive(Debug, Clone, PartialEq, Eq, RustcEncodable, RustcDecodable)]
pub struct User {
    pub nick: String,
    pub ident: String,
    pub host: String,
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
