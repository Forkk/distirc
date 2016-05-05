//! Data structures for managing users and their IRC networks.

use std::collections::HashMap;
use std::collections::hash_map;
use std::default::Default;

use common::types::NetId;

use network::IrcNetwork;
use config::{UserConfig, NetConfig};
pub use config::UserId;


/// This struct stores the state for a particular IRC user.
///
/// This is mainly used as a container object for the user's configuration and
/// network list. Methods for sending things to the user's clients can be found
/// in `state::UserHandle`.
// #[derive(Debug)]
pub struct User {
    pub cfg: UserConfig,
    networks: HashMap<NetId, IrcNetwork>,
}

impl User {
    /// Constructs a new user with a base config with no networks.
    pub fn new() -> User {
        User {
            cfg: UserConfig::default(),
            networks: HashMap::new(),
        }
    }

    /// Constructs a new user from the given configuration object.
    pub fn from_cfg(cfg: UserConfig) -> User {
        let mut us = Self::new();
        for (name, net_cfg) in cfg.net.iter() {
            us.add_network(name.clone(), net_cfg);
        }
        us.cfg = cfg;
        us
    }

    pub fn add_network(&mut self, name: String, cfg: &NetConfig) {
        self.networks.insert(name.to_owned(), IrcNetwork::new(name, cfg));
    }


    /// Returns an iterator over this user's IRC networks.
    pub fn iter_nets(&self) -> IterNets {
        self.networks.iter()
    }

    /// Gets a reference to a network with the given ID if it exists.
    pub fn get_net(&self, id: &NetId) -> Option<&IrcNetwork> {
        self.networks.get(id)
    }

    /// Gets a mutable reference to a network with the given ID if it exists.
    pub fn get_net_mut(&mut self, id: &NetId) -> Option<&mut IrcNetwork> {
        self.networks.get_mut(id)
    }
}

pub type IterNets<'a> = hash_map::Iter<'a, NetId, IrcNetwork>;
