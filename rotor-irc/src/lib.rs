#[macro_use] extern crate log;
extern crate rotor;
extern crate rotor_stream;

mod response;
mod message;
mod machine;

pub use message::{Message, Command, ParseError};
pub use response::Response;
pub use machine::{IrcConnection, IrcMachine, IrcAction};
