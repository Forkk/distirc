//! This module implements the server socket.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::Sender;
use std::net::ToSocketAddrs;
use rotor::{Machine, Response, Scope, EventSet, Notifier};
use rotor::void::Void;
use rotor::mio::tcp::TcpStream;
use rotor_stream::Stream;
use rotor_irc::{IrcConnection};

use common::conn::Handler;
use common::messages::{CoreMsg, NetId};

use user::UserState;
use config::{UserConfig, UserId};
use handle::BaseUpdateHandle;

mod client;
pub mod irc;

use self::irc::IrcNetConn;
pub use self::client::{Client};


pub struct User {
    pub state: UserState,
    clients: UserClients,
}

impl User {
    /// Sends messages and alerts bufferred in the given update handle.
    pub fn send_handle_msgs(&mut self, mut u: BaseUpdateHandle<CoreMsg>) {
        for msg in u.take_msgs() {
            self.clients.broadcast(&msg);
        }
        if !self.clients.0.is_empty() {
            let alerts = self.state.take_alerts();
            self.clients.broadcast(&CoreMsg::Alerts(alerts));
        } else if let Some(ref cmd) = self.state.cfg.alert_cmd.clone() {
            use std::process::Command;
            for alert in self.state.take_alerts() {
                let cmd = cmd.replace("%m", &alert.msg);
                info!("Sending alert with command {}", cmd);
                Command::new("/bin/sh").arg("-c").arg(cmd).spawn().expect("Failed to spawn alert command");
            }
        }
    }
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
    pub users: HashMap<UserId, User>,
    /// Notifier to spawn new connections
    pub notif: Notifier,
    pub spawn_conns: VecDeque<(UserId, NetId)>,
}

impl Context {
    pub fn new(notif: Notifier) -> Context {
        Context {
            users: HashMap::new(),
            notif: notif,
            spawn_conns: VecDeque::new(),
        }
    }

    pub fn spawn_conn(&mut self, uid: UserId, nid: NetId) {
        self.spawn_conns.push_back((uid, nid));
        self.notif.wakeup().unwrap();
    }

    pub fn add_user(&mut self, name: &str, cfg: UserConfig) {
        let state = UserState::from_cfg(cfg);
        self.users.insert(name.to_owned(), User {
            state: state,
            clients: UserClients(vec![]),
        });
    }

    /// Spawns IRC connections for all users.
    pub fn spawn_conns(&mut self) {
        for (uid, usr) in self.users.iter() {
            for (nid, _) in usr.state.iter_nets() {
                self.spawn_conns.push_back((uid.clone(), nid.clone()));
            }
        }
        self.notif.wakeup().unwrap();
    }
}


/// State machine that handles spawning IRC connections.
///
/// This machine is responsible for spawning IRC server connections. When the
/// machine is notified by the `notif` field in `Context`, it wakes up and looks
/// in `spawn_conns` and spawns connections from there.
pub enum ConnSpawner {
    Spawner,
    Conn(Stream<IrcConnection<IrcNetConn>>),
}

impl Machine for ConnSpawner {
    type Context = Context;
    type Seed = (UserId, NetId);

    fn create(seed: Self::Seed, scope: &mut Scope<Context>) -> Response<Self, Void> {
        let addr = if let Some(usr) = scope.users.get_mut(&seed.0) {
            if let Some(net) = usr.state.get_network_mut(&seed.1) {
                let result = (net.cfg.server(), net.cfg.port()).to_socket_addrs()
                    .map(|mut iter| iter.next().unwrap());
                match result {
                    Ok(addr) => addr,
                    Err(e) => {
                        error!("Error parsing network address for network {}: {:?}", &seed.1, e);
                        return Response::done();
                    }
                }
            } else {
                error!("Tried to spawn connection for nonexistant network");
                return Response::done();
            }
        } else {
            error!("Tried to spawn connection for nonexistant user");
            return Response::done();
        };

        match TcpStream::connect(&addr) {
            Ok(sock) => Stream::new(sock, seed, scope)
                .map(ConnSpawner::Conn, |_| unreachable!("Connection spawned machine")),
            Err(e) => {
                error!("Error connecting to IRC server for user {} on network {}: {}",
                       seed.0, seed.1, e);
                Response::done()
            },
        }
    }

    fn spawned(self, s: &mut Scope<Context>) -> Response<Self, Self::Seed> {
        match self {
            ConnSpawner::Spawner => Response::ok(self),
            ConnSpawner::Conn(conn) => {
                conn.spawned(s).map(ConnSpawner::Conn, |_| unreachable!("Connection spawned machine"))
            },
        }
    }

    fn ready(self, e: EventSet, s: &mut Scope<Context>) -> Response<Self, Self::Seed> {
        match self {
            ConnSpawner::Spawner => unreachable!(),
            ConnSpawner::Conn(conn) => {
                conn.ready(e, s).map(ConnSpawner::Conn, |_| unreachable!("Connection spawned machine"))
            },
        }
    }

    fn timeout(self, scope: &mut Scope<Context>) -> Response<Self, Self::Seed> {
        match self {
            ConnSpawner::Spawner => unreachable!(),
            ConnSpawner::Conn(conn) => {
                conn.timeout(scope).map(ConnSpawner::Conn, |_| unreachable!("Connection spawned machine"))
            },
        }
    }

    fn wakeup(self, scope: &mut Scope<Context>) -> Response<Self, Self::Seed> {
        match self {
            ConnSpawner::Spawner => {
                trace!("Spawner woke up");
                if let Some(seed) = scope.spawn_conns.pop_front() {
                    info!("Spawning IRC connection for user {}'s network {}", seed.0, seed.1);
                    // If there are still more connections to spawn, we wake ourself up
                    // again so we can spawn them.
                    if !scope.spawn_conns.is_empty() {
                        scope.notif.wakeup().unwrap();
                    }
                    Response::spawn(ConnSpawner::Spawner, seed)
                } else {
                    Response::ok(ConnSpawner::Spawner)
                }
            },
            ConnSpawner::Conn(conn) => {
                conn.wakeup(scope).map(ConnSpawner::Conn, |_| unreachable!("Connection spawned machine"))
            },
        }
    }
}
