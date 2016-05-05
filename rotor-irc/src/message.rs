//! Defines the `Message` data structure and friends.

use std::fmt;
use std::error::Error;
use std::str::FromStr;

use response::Response;

/// Represents an IRC message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Optional message prefix.
    pub prefix: Option<String>,
    /// The IRC command.
    pub command: Command,
    pub args: Vec<String>,
    pub body: Option<String>,
}

impl Message {
    pub fn new(prefix: Option<String>, cmd: Command, args: Vec<String>, body: Option<String>) -> Message {
        Message {
            prefix: prefix,
            command: cmd,
            args: args,
            body: body,
        }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref pfx) = self.prefix {
            try!(write!(f, ":{} ", pfx));
        }
        try!(write!(f, "{}", self.command));
        for arg in self.args.iter() {
            try!(write!(f, " {}", arg));
        }
        if let Some(ref body) = self.body {
            try!(write!(f, " :{}", body));
        }
        Ok(())
    }
}


macro_rules! irc_commands {
    ( $( $name: ident ),*, ) => {
        /// Represents IRC commands as defined by
        /// [RFC 2812](http://tools.ietf.org/html/rfc2812#section-3).
        #[derive(Debug, Clone, PartialEq)]
        pub enum Command {
            $( $name ),*,
            /// Represents a numeric response code.
            Response(Response),
            Other(String),
        }

        impl FromStr for Command {
            type Err = ParseError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $( stringify!($name) => Ok(Command::$name)),*,
                    s => {
                        match s.parse::<u16>() {
                            Ok(c) => Ok(Command::Response(Response::from_u16(c))),
                            Err(_) => Ok(Command::Other(s.to_owned())),
                        }
                    },
                }
            }
        }

        impl fmt::Display for Command {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match *self {
                    $( Command::$name => write!(f, stringify!($name)) ),*,
                    Command::Response(r) => write!(f, "{}", r),
                    Command::Other(ref s) => write!(f, "{}", s),
                }
            }
        }
    };
}

irc_commands! {
    // 3.1 Connection Registration
    PASS,
    NICK,
    USER,
    OPER,
    MODE,
    SERVICE,
    QUIT,
    SQUIT,

    // 3.2 Channel Operations
    JOIN,
    PART,
    // MODE already defined
    TOPIC,
    NAMES,
    LIST,
    INVITE,
    KICK,

    // 3.3 Sending Messages
    PRIVMSG,
    NOTICE,

    // 3.4 Server Queries and Commands
    // We omit most of these as they aren't relevant for clients or won't be used.
    MOTD,

    // 3.5 Service Query and Commands
    // Again, these are omitted.

    // 3.6 User Based Queries
    WHO,
    WHOIS,
    WHOWAS,

    // 3.7 Miscellaneous Messages
    KILL,
    PING,
    PONG,

    // 4 Optional Features
    AWAY,
    ISON,
}


impl FromStr for Message {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (s, prefix) = if s.starts_with(":") {
            let pfx_end = try!(s.find(" ").ok_or(ParseError::UnexpectedEnd));
            (&s[pfx_end+1..], Some(s[1..pfx_end].to_owned()))
        } else { (s, None) };

        let (s, cmd) = if let Some(end) = s.find(" ") {
            (&s[end+1..], &s[..end])
        } else {
            ("", s)
        };

        if cmd.is_empty() {
            return Err(ParseError::EmptyCommand);
        }
        let cmd = try!(cmd.parse::<Command>());

        let (s, suffix) = if let Some(start) = s.find(":") {
            let next_s = if start > 0 { &s[..start-1] } else { "" };
            (next_s, Some(s[start+1..].to_owned()))
        } else {
            (s, None)
        };

        let args: Vec<_> = if !s.is_empty() {
            s.split(" ").map(|s| s.to_owned()).collect()
        } else { vec![] };

        Ok(Message {
            prefix: prefix,
            args: args,
            command: cmd,
            body: suffix,
        })
    }
}


/// An error that might occur when parsing an IRC message.
#[derive(Debug, Clone)]
pub enum ParseError {
    // /// The IRC command was invalid. This doesn't necessarily mean it was
    // /// unrecognized, it could indicate that it contained invalid characters.
    // InvalidCommand,
    UnexpectedEnd,
    EmptyCommand,
}

impl Error for ParseError {
    fn description(&self) -> &str {
        use self::ParseError::*;
        match *self {
            // InvalidCommand => "Invalid IRC command",
            UnexpectedEnd => "Command ended unexpectedly",
            EmptyCommand => "Command was blank",
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}


#[cfg(test)]
mod tests {
    use super::{Message, Command};
    use response::Response;

    macro_rules! parse_fmt_test {
        ( $parse_name: ident, $fmt_name: ident, $body: block ) => {
            #[test]
            fn $parse_name() {
                let (s, msg) = $body;
                assert_eq!(s.parse::<Message>().unwrap(), msg)
            }

            #[test]
            fn $fmt_name() {
                let (s, msg) = $body;
                assert_eq!(&msg.to_string(), s);
            }
        }
    }

    parse_fmt_test!(parse_basic, format_basic, {
        let s = "PING irc.server.lol";
        let msg = Message {
            prefix: None,
            command: Command::PING,
            args: vec!["irc.server.lol".to_owned()],
            body: None,
        };
        (s, msg)
    });

    parse_fmt_test!(parse_prefix, format_prefix, {
        let s = ":guy!~ident@some.host JOIN #code";
        let msg = Message {
            prefix: Some("guy!~ident@some.host".to_owned()),
            command: Command::JOIN,
            args: vec!["#code".to_owned()],
            body: None,
        };
        (s, msg)
    });

    parse_fmt_test!(parse_body, format_body, {
        let s = "PRIVMSG #code :Rust is the best language ever";
        let msg = Message {
            prefix: None,
            command: Command::PRIVMSG,
            args: vec!["#code".to_owned()],
            body: Some("Rust is the best language ever".to_owned()),
        };
        (s, msg)
    });

    parse_fmt_test!(parse_body_no_args, format_body_no_args, {
        let s = "PRIVMSG :Rust is the best language ever";
        let msg = Message {
            prefix: None,
            command: Command::PRIVMSG,
            args: vec![],
            body: Some("Rust is the best language ever".to_owned()),
        };
        (s, msg)
    });

    parse_fmt_test!(parse_body_and_prefix, format_body_and_prefix, {
        let s = ":forkk!~forkk@forkk.net PRIVMSG #code :Rust is the best language ever";
        let msg = Message {
            prefix: Some("forkk!~forkk@forkk.net".to_owned()),
            command: Command::PRIVMSG,
            args: vec!["#code".to_owned()],
            body: Some("Rust is the best language ever".to_owned()),
        };
        (s, msg)
    });

    parse_fmt_test!(parse_response, format_response, {
        let s = ":fake.irc.server 001 #code :Rust is the best language ever";
        let msg = Message {
            prefix: Some("fake.irc.server".to_owned()),
            command: Command::Response(Response::RPL_WELCOME),
            args: vec!["#code".to_owned()],
            body: Some("Rust is the best language ever".to_owned()),
        };
        (s, msg)
    });
}
