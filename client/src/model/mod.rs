//! This module and its submodules implement the "model" component of the
//! client. That is, the back-end parts that connect to the server.

mod buffer;

pub use self::buffer::{BufHandle, BufferLine};
