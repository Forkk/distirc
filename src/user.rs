//! This module implements distirc's multi-user system. Each user on the core
//! has its own thread which connects to that user's IRC networks and handles a
//! set of clients for the user.

use std::thread;
use std::thread::JoinHandle;
// use std::sync::mpsc::{channel};
use std::collections::HashMap;
use irc::client::prelude::*;

use network::IrcNetwork;
use config::{UserConfig, IrcNetConfig};

// TODO: Move config stuff elsewhere
/// This is a wrapper around a user thread.
///
/// It implements an interface for communicating information about the user to
/// the main thread.
pub struct UserThread {
    hand: JoinHandle<()>,
}

impl UserThread {
    /// Spawns a new user thread with the given configuration.
    pub fn spawn(cfg: &UserConfig) -> UserThread {
        let cfg2 = cfg.clone();
        let hand = thread::Builder::new()
                       .name(format!("user-{}", cfg.name))
                       .spawn(move || {
                           run_user_thread(cfg2);
                       })
                       .expect("Failed to spawn user thread");
        UserThread { hand: hand }
    }

    pub fn join(self) -> thread::Result<()> {
        self.hand.join()
    }
}


struct UserThreadState {
    networks: HashMap<String, IrcNetwork>,
}

impl UserThreadState {
    fn new() -> UserThreadState {
        UserThreadState { networks: HashMap::new() }
    }

    fn add_server(&mut self, name: &str, cfg: &IrcNetConfig) {
        self.networks.insert(name.to_owned(), IrcNetwork::new(cfg));
    }

    fn init(&mut self) {
        for (_, mut net) in self.networks.iter_mut() {
            net.connect();
        }
    }

    /// Process messages from servers and clients
    fn update(&mut self) {
        for (_, serv) in self.networks.iter_mut() {
            serv.update();
        }
    }
}

fn run_user_thread(cfg: UserConfig) {
    info!("Starting user thread");

    let mut ts = UserThreadState::new();

    for (name, net_cfg) in cfg.networks.iter() {
        ts.add_server(&name, net_cfg);
    }

    ts.init();
    'main: loop {
        ts.update();
        thread::sleep_ms(50);
    }
}
