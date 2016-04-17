//! This module implements the server socket.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use rotor::{Machine, Response, Scope, EventSet, Notifier};
use rotor::void::Void;

use common::conn::{Handler, Action};
use common::messages::{
    CoreMsg,
    ClientMsg, ClientNetMsg, ClientBufMsg,
};

use user::UserState;
use config::{UserConfig, UserId};
use network::{IrcNetwork, BufHandle};


struct User {
    state: UserState,
    clients: UserClients,
}

struct UserClients(Vec<UserClient>);

impl UserClients {
    /// Broadcasts the given message to all this user's clients.
    ///
    /// As a side-effect, this function will also prune any disconnected clients
    /// (clients whose `Receiver`) has been `drop`ed.
    fn broadcast(&mut self, msg: &CoreMsg) {
        self.0.retain(|client| {
            if let Err(_) = client.tx.send(msg.clone()) {
                return false;
            }
            client.wake.wakeup().unwrap();
            true
        });
    }
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
            clients: UserClients(vec![]),
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

        usr.clients.0.push(UserClient {
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
    fn msg_recv(self, msg: &Self::Recv, s: &mut Scope<Self::Context>) -> Action<Self> {
        info!("Received message: {:?}", msg);
        match self {
            Client::Connected(uid, rx) => {
                let mut user = match s.users.get_mut(&uid) {
                    Some(u) => u,
                    None => {
                        error!("Unknown user state: {}", uid);
                        return Action::done();
                    },
                };
                Client::Connected(uid, rx).handle_user_msg(msg, &mut user)
            }
        }
    }

    /// A timeout occurred.
    fn timeout(self, _scope: &mut Scope<Self::Context>) -> Action<Self> {
        unreachable!("Unexpected timeout")
    }

    fn wakeup(self, _s: &mut Scope<Self::Context>) -> Action<Self> {
        trace!("Client woke up");
        match self {
            Client::Connected(uid, rx) => {
                // Send new messages to the client.
                let mut msgs = vec![];
                while let Ok(msg) = rx.try_recv() {
                    trace!("Sending client message: {:?}", msg);
                    msgs.push(msg);
                }
                let mut a = Action::ok(Client::Connected(uid, rx));
                a = a.send_all(msgs.clone());
                a
            }
        }
    }
}

impl Client {
    fn handle_user_msg(self, msg: &ClientMsg, user: &mut User) -> Action<Self> {
        match *msg {
            ClientMsg::NetMsg(ref nid, ref msg) => {
                if let Some(ref mut net) = user.state.get_network_mut(&nid) {
                    self.handle_net_msg(msg, net, &mut user.clients)
                } else {
                    Action::ok(self)
                }
            },
            ClientMsg::BufMsg(ref _bid, ref _msg) => {
                warn!("Global buffer message routing unimplemented");
                Action::ok(self)
            },
            ClientMsg::ListGlobalBufs => {
                warn!("Global buffer message routing unimplemented");
                Action::ok(self)
            },
            ClientMsg::ListNets => {
                let mut nets = vec![];
                for (_nid, net) in user.state.iter_nets() {
                    nets.push(net.to_info());
                }

                Action::ok(self).send(CoreMsg::Networks(nets))
            },
        }
    }

    fn handle_net_msg(self, msg: &ClientNetMsg, net: &mut IrcNetwork, clients: &mut UserClients) -> Action<Self> {
        match *msg {
            ClientNetMsg::BufMsg(ref targ, ref msg) => {
                if let Some(mut buf) = net.get_buf_mut(&targ) {
                    self.handle_buf_msg(msg, &mut buf, clients)
                } else {
                    warn!("Ignoring message for unknown buffer {:?}. Message: {:?}", targ, msg);
                    Action::ok(self)
                }
            },
            ClientNetMsg::ListBufs => {
                warn!("ListBufs not implemented");
                Action::ok(self)
            },
            ClientNetMsg::JoinChan(ref chan) => {
                net.join_chan(chan);
                Action::ok(self)
            },
        }
    }

    fn handle_buf_msg(self,
                      msg: &ClientBufMsg,
                      buf: &mut BufHandle,
                      clients: &mut UserClients)
                      -> Action<Self>
    {
        match *msg {
            ClientBufMsg::FetchLogs { .. } => {
                warn!("TODO: Handle FetchLogs");
                Action::ok(self)
            },
            ClientBufMsg::SendMsg(ref msg) => {
                buf.send_privmsg(msg.clone(), &mut |m| {
                    clients.broadcast(&m)
                });
                Action::ok(self)
            },
            ClientBufMsg::PartChan(ref optmsg) => {
                buf.part_chan(optmsg);
                Action::ok(self)
            },
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
            for msg in msgs {
                user.clients.broadcast(&msg);
            }
        }
        Response::ok(self)
    }
}
