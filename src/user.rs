//! This module implements distirc's multi-user system. Each user on the core
//! has its own thread which connects to that user's IRC networks and handles a
//! set of clients for the user.

// use std::sync::mpsc::{channel};
use std::collections::HashMap;
use std::collections::hash_map;
use std::default::Default;
use irc::client::prelude::*;
use rotor::Notifier;

use common::messages::CoreMsg;
use common::types::NetId;
use common::alert::Alert;

use network::IrcNetwork;
use config::{UserConfig, NetConfig};
use handle::{BaseUpdateHandle};


// #[derive(Debug)]
pub struct UserState {
    pub cfg: UserConfig,
    networks: HashMap<NetId, IrcNetwork>,
    wake: Notifier,
    /// Queue for alerts that happened while no client was connected.
    alerts: Vec<Alert>,
}

impl UserState {
    fn new(wake: Notifier) -> UserState {
        UserState {
            cfg: UserConfig::default(),
            networks: HashMap::new(),
            wake: wake,
            alerts: vec![],
        }
    }

    pub fn from_cfg(wake: Notifier, cfg: UserConfig) -> UserState {
        let mut us = Self::new(wake);
        for (name, net_cfg) in cfg.net.iter() {
            us.add_server(&name, net_cfg);
        }
        us.cfg = cfg;
        us
    }

    fn add_server(&mut self, name: &str, cfg: &NetConfig) {
        self.networks.insert(name.to_owned(), IrcNetwork::new(name, cfg));
    }

    pub fn init(&mut self) {
        for (_, mut net) in self.networks.iter_mut() {
            net.connect(self.wake.clone()).unwrap(); // FIXME: Handle this error
        }
    }

    /// Process messages from servers and clients
    pub fn update(&mut self, msgs: &mut Vec<CoreMsg>) {
        let mut u = BaseUpdateHandle::<CoreMsg>::new();
        for (_, serv) in self.networks.iter_mut() {
            serv.update(&mut u);
        }
        self.alerts.append(&mut u.take_alerts());
        msgs.append(&mut u.take_msgs());
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
