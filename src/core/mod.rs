//! This module implements the server socket.

use std::collections::HashMap;
use rotor::Scope;

use user::UserState;
use config::{UserConfig, UserId};
use common::conn::{Handler, Action};
use common::messages::{CoreMsg, ClientMsg};


// #[derive(Debug)]
pub struct Context {
    users: HashMap<UserId, UserState>,
}

impl Context {
    pub fn new() -> Context {
        Context { users: HashMap::new() }
    }

    pub fn add_user(&mut self, name: &str, cfg: UserConfig) {
        let user = UserState::from_cfg(cfg);
        self.users.insert(name.to_owned(), user);
    }

    /// Initializes users, connecting them to their networks.
    pub fn init(&mut self) {
        for (_, mut user) in self.users.iter_mut() {
            user.init();
        }
    }
}


/// This machine handles a client's state.
pub enum Client {
    // /// The client has just connected and hasn't authenticated yet.
    // Connecting,
    /// The client has authenticated as a user.
    Connected(UserId),
}

impl Handler for Client {
    type Context = Context;
    type Send = CoreMsg;
    type Recv = ClientMsg;

    fn create(s: &mut Scope<Self::Context>) -> Action<Self> {
        let uid = "Forkk";
        let me = Client::Connected(uid.to_owned());

        let user = match s.users.get(uid) {
            Some(u) => u,
            None => {
                error!("Unknown user state: {}", uid);
                return Action::done();
            },
        };

        // Send the networks list.
        let mut nets = vec![];
        for (_nid, net) in user.iter_nets() {
            nets.push(net.to_info());
        }

        Action::ok(me).send(CoreMsg::Networks(nets))
    }

    /// A message has been received.
    fn msg_recv(self, msg: &Self::Recv, _s: &mut Scope<Self::Context>) -> Action<Self> {
        info!("Received message: {:?}", msg);
        Action::ok(self)
    }

    /// A timeout occurred.
    fn timeout(self, _scope: &mut Scope<Self::Context>) -> Action<Self> {
        unreachable!("Unexpected timeout")
    }

    fn wakeup(self, _scope: &mut Scope<Self::Context>) -> Action<Self> {
        unreachable!("Unexpected wakeup")
    }
}
