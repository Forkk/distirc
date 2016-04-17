pub use self::core::{CoreMsg, CoreNetMsg, CoreBufMsg};
pub use self::client::{ClientMsg, ClientNetMsg, ClientBufMsg};

pub use line::BufferLine;
pub use types::{NetId, BufId};

// Auxillary data structures

/// Short summary data used to tell a client about a network.
#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct NetInfo {
    pub name: String,
    pub buffers: Vec<BufInfo>,
}

/// Short summary data used to tell a client about a buffer.
#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct BufInfo {
    pub name: String,
}


/// Identifies a buffer.
#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub enum BufTarget {
    /// An IRC channel buffer
    Channel(BufId),
    /// An IRC private message buffer with the given user.
    Private(BufId),
    /// The network's status buffer.
    Network,
}

impl BufTarget {
    pub fn name(&self) -> &str {
        match *self {
            BufTarget::Channel(ref n) => n,
            BufTarget::Private(ref n) => n,
            BufTarget::Network => "*network*",
        }
    }
}


// Message types

mod core {
    use line::BufferLine;
    use types::{NetId, BufId};
    use super::{BufTarget, NetInfo, BufInfo};

    /// Messages sent from the core.
    #[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
    pub enum CoreMsg {
        /// This message tells the client about a list of networks and their buffers.
        Networks(Vec<NetInfo>),

        /// Tells the client about global buffers.
        GlobalBufs(Vec<BufInfo>),

        /// Wrapper for messages about a specific network.
        NetMsg(NetId, CoreNetMsg),

        /// Wrapper for messages about a global buffer.
        BufMsg(BufId, CoreBufMsg),
    }

    /// Messages sent from the core about a specific network.
    #[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
    pub enum CoreNetMsg {
        /// Tells the client about changes in the network's state.
        State {
            connected: bool,
        },

        /// Wrapper for messages about a buffer within the network.
        BufMsg(BufTarget, CoreBufMsg),

        /// Tells the client about a list of buffers within the network.
        Buffers(Vec<BufInfo>),
    }

    /// Messages sent from the core about a specific buffer.
    #[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
    pub enum CoreBufMsg {
        /// Tells the client about the buffer's state.
        State {
            /// For channel buffers, this indicates whether the user is in the
            /// channel. For private message buffers, this indicates if the
            /// other user is online or not.
            joined: bool,
        },

        /// New lines have been posted to the bottom of the buffer.
        ///
        /// This is for messages that have just been received, not for requested
        /// scrollback. Lines are sent with the oldest first.
        NewLines(Vec<BufferLine>),

        /// Used to send scrollback. These lines should be appended to the top
        /// of the buffer.
        Scrollback(Vec<BufferLine>),
    }
}

mod client {
    use types::{NetId, BufId};
    use super::BufTarget;

    /// Messages sent from the client.
    #[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
    pub enum ClientMsg {
        /// Wrapper for messages about a network.
        NetMsg(NetId, ClientNetMsg),

        /// Wrapper for messages about a global buffer.
        BufMsg(BufId, ClientBufMsg),

        /// Requests that the server re-send the network list.
        ListNets,

        /// Requests that the server re-send the global buffers list.
        ListGlobalBufs,
    }

    /// Messages from the client about a network.
    #[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
    pub enum ClientNetMsg {
        /// Wrapper for messages about a channel in this network.
        BufMsg(BufTarget, ClientBufMsg),

        /// Requests that the server re-send the buffer list for this network.
        ListBufs,
    }

    /// Messages from the client about a buffer.
    #[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
    pub enum ClientBufMsg {
        /// Sends a message to the buffer.
        SendMsg(String),

        /// Requests that the server send the client `count` many lines of
        /// scrollback starting from the `start`th line from the bottom of the
        /// buffer.
        ///
        /// The server may or may not honor the `count` field for various
        /// reasons. If `count` is too large, the server may ignore it for flood
        /// protection reasons. It may also be dishonored if there aren't that
        /// many lines of scrollback.
        FetchLogs {
            // NOTE: If, by the time this message is received, more lines have
            // been appended to the buffer, start will be off by however many
            // lines have been added, possibly resulting in some lines being
            // missed. For now, this isn't worth dealing with, but it may be a
            // good idea to fix it later.
            start: usize,
            count: usize,
        },
    }
}
