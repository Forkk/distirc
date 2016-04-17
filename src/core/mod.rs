//! This module implements the server socket.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use rotor::{Machine, Response, Scope, EventSet, Notifier};
use rotor::void::Void;

use user::UserState;
use config::{UserConfig, UserId};
use common::conn::{Handler, Action};
use common::messages::{CoreMsg, ClientMsg};


struct User {
    state: UserState,
    clients: Vec<UserClient>,
}

struct UserClient {
    wake: Notifier,
    tx: Sender<CoreMsg>,
}

// #[derive(Debug)]
pub struct Context {
    users: HashMap<UserId, User>,
    /// Notifier to update the users' state.
    notif: Notifier,
}

impl Context {
    pub fn new(notif: Notifier) -> Context {
        Context {
            users: HashMap::new(),
            notif: notif,
        }
    }

    pub fn add_user(&mut self, name: &str, cfg: UserConfig) {
        let state = UserState::from_cfg(self.notif.clone(), cfg);
        self.users.insert(name.to_owned(), User {
            state: state,
            clients: vec![],
        });
    }

    /// Initializes users, connecting them to their networks.
    pub fn init(&mut self) {
        for (_, ref mut user) in self.users.iter_mut() {
            user.state.init();
        }
    }
}


/// This machine handles a client's state.
pub enum Client {
    // /// The client has just connected and hasn't authenticated yet.
    // Connecting,
    /// The client has authenticated as a user.
    Connected(UserId, Receiver<CoreMsg>),
}

impl Handler for Client {
    type Context = Context;
    type Send = CoreMsg;
    type Recv = ClientMsg;

    fn create(s: &mut Scope<Self::Context>) -> Action<Self> {
        let (tx, rx) = channel();

        let uid = "Forkk";
        let me = Client::Connected(uid.to_owned(), rx);

        let notif = s.notifier();

        let usr = match s.users.get_mut(uid) {
            Some(u) => u,
            None => {
                error!("Unknown user state: {}", uid);
                return Action::done();
            },
        };

        usr.clients.push(UserClient {
            wake: notif,
            tx: tx,
        });

        // Send the networks list.
        let mut nets = vec![];
        for (_nid, net) in usr.state.iter_nets() {
            nets.push(net.to_info());
        }

        Action::ok(me).send(CoreMsg::Networks(nets))
    }

    /// A message has been received.
    fn msg_recv(self, msg: &Self::Recv, _s: &mut Scope<Self::Context>) -> Action<Self> {
        info!("Received message: {:?}", msg);
        // match self {
        //     Client::Connected(uid) => {
        //         let utrp = match s.users.get(uid) {
        //             Some(u) => u,
        //             None => {
        //                 error!("Unknown user state: {}", uid);
        //                 return Action::done();
        //             },
        //         };
        //         let (ref mut user, _, _) = utrp;
        //     }
        // }
        Action::ok(self)
    }

    /// A timeout occurred.
    fn timeout(self, _scope: &mut Scope<Self::Context>) -> Action<Self> {
        unreachable!("Unexpected timeout")
    }

    fn wakeup(self, s: &mut Scope<Self::Context>) -> Action<Self> {
        trace!("Client woke up");
        match self {
            Client::Connected(uid, rx) => {
                let mut msgs = vec![];
                while let Ok(msg) = rx.try_recv() { msgs.push(msg); }
                let mut a = Action::ok(Client::Connected(uid, rx));
                a = a.send_all(msgs.clone());
                a
            }
        }
    }
}


/// State machine that updates networks when a message is received.
pub struct Updater;

impl Machine for Updater {
    type Context = Context;
    type Seed = ();

    fn create(_seed: (), _s: &mut Scope<Context>) -> Response<Self, Void> {
        Response::ok(Updater)
    }

    fn spawned(self, _s: &mut Scope<Context>) -> Response<Self, ()> {
        Response::ok(self)
    }

    fn ready(self, _e: EventSet, _s: &mut Scope<Context>) -> Response<Self, ()> {
        Response::ok(self)
    }

    fn timeout(self, _s: &mut Scope<Context>) -> Response<Self, ()> {
        Response::ok(self)
    }

    fn wakeup(self, scope: &mut Scope<Context>) -> Response<Self, ()> {
        trace!("Updater woke up");
        for (_uid, ref mut user) in scope.users.iter_mut() {
            let mut msgs = vec![];
            user.state.update(&mut msgs);
            if !msgs.is_empty() {
                user.clients.retain(|client| {
                    for msg in msgs.iter() {
                        if let Err(_) = client.tx.send(msg.clone()) {
                            return false;
                        }
                    }
                    client.wake.wakeup().unwrap();
                    true
                });
            }
        }
        Response::ok(self)
    }
}
