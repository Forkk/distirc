//! This module implements distirc's multi-user system. Each user on the core
//! has its own thread which connects to that user's IRC networks and handles a
//! set of clients for the user.

// use std::sync::mpsc::{channel};
use std::collections::HashMap;
use std::collections::hash_map;
use irc::client::prelude::*;

use common::types::NetId;

use network::IrcNetwork;
use config::{UserConfig, IrcNetConfig};


// #[derive(Debug)]
pub struct UserState {
    networks: HashMap<NetId, IrcNetwork>,
}

impl UserState {
    pub fn new() -> UserState {
        UserState { networks: HashMap::new() }
    }

    pub fn from_cfg(cfg: UserConfig) -> UserState {
        let mut us = Self::new();
        for (name, net_cfg) in cfg.networks.iter() {
            us.add_server(&name, net_cfg);
        }
        us
    }

    fn add_server(&mut self, name: &str, cfg: &IrcNetConfig) {
        self.networks.insert(name.to_owned(), IrcNetwork::new(cfg));
    }

    pub fn init(&mut self) {
        for (_, mut net) in self.networks.iter_mut() {
            net.connect().unwrap(); // FIXME: Handle this error
        }
    }

    /// Process messages from servers and clients
    pub fn update(&mut self) {
        for (_, serv) in self.networks.iter_mut() {
            serv.update();
        }
    }

    pub fn iter_nets(&self) -> IterNets {
        self.networks.iter()
    }

    pub fn get_network(&self, id: &NetId) -> Option<&IrcNetwork> {
        self.networks.get(id)
    }
}

pub type IterNets<'a> = hash_map::Iter<'a, NetId, IrcNetwork>;
