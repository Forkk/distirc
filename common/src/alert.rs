//! Common types for sending alerts between client and server.

use types::{NetId, BufId};

include!(concat!(env!("OUT_DIR"), "/alert.rs"));
