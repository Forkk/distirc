//! This module implements distirc's multi-user system. Each user on the core
//! has its own thread which connects to that user's IRC networks and handles a
//! set of clients for the user.

// use std::sync::mpsc::{channel};
use std::collections::HashMap;
use std::collections::hash_map;
use std::default::Default;

use common::types::NetId;
use common::alert::Alert;

use network::IrcNetwork;
use config::{UserConfig, NetConfig};


// #[derive(Debug)]
pub struct UserState {
    pub cfg: UserConfig,
    networks: HashMap<NetId, IrcNetwork>,
    /// Queue for alerts that happened while no client was connected.
    alerts: Vec<Alert>,
}

impl UserState {
    fn new() -> UserState {
        UserState {
            cfg: UserConfig::default(),
            networks: HashMap::new(),
            alerts: vec![],
        }
    }

    pub fn from_cfg(cfg: UserConfig) -> UserState {
        let mut us = Self::new();
        for (name, net_cfg) in cfg.net.iter() {
            us.add_network(name.clone(), net_cfg);
        }
        us.cfg = cfg;
        us
    }

    fn add_network(&mut self, name: String, cfg: &NetConfig) {
        self.networks.insert(name.to_owned(), IrcNetwork::new(name, cfg));
    }

    pub fn iter_nets(&self) -> IterNets {
        self.networks.iter()
    }

    pub fn get_network(&self, id: &NetId) -> Option<&IrcNetwork> {
        self.networks.get(id)
    }

    pub fn get_network_mut(&mut self, id: &NetId) -> Option<&mut IrcNetwork> {
        self.networks.get_mut(id)
    }

    pub fn take_alerts(&mut self) -> Vec<Alert> {
        use std::mem;
        let mut alerts = vec![];
        mem::swap(&mut alerts, &mut self.alerts);
        alerts
    }
}

pub type IterNets<'a> = hash_map::Iter<'a, NetId, IrcNetwork>;
