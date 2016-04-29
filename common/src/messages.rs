use std::fmt;

pub use self::core::{CoreMsg, CoreNetMsg, CoreBufMsg};
pub use self::client::{ClientMsg, ClientNetMsg, ClientBufMsg};

pub use line::{BufferLine, MsgKind};
pub use types::{NetId, BufId};
pub use alert::Alert;

include!(concat!(env!("OUT_DIR"), "/messages.rs"));
