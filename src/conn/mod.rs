//! This module implements the server socket.

use std::collections::VecDeque;
use std::net::ToSocketAddrs;
use rotor::{Machine, Response, Scope, EventSet, Notifier};
use rotor::void::Void;
use rotor::mio::tcp::TcpStream;
use rotor_stream::Stream;
use rotor_irc::IrcConnection;

use common::conn::Handler;
use common::messages::{NetId};

use state::Core;
use config::UserId;

mod client;
pub mod irc;

use self::irc::IrcNetConn;
pub use self::client::{Client};


// #[derive(Debug)]
pub struct Context {
    /// Holds the core state
    pub core: Core,
    /// Notifier to spawn new connections
    pub notif: Notifier,
    pub spawn_conns: VecDeque<(UserId, NetId)>,
}

impl Context {
    pub fn new(notif: Notifier) -> Context {
        Context {
            core: Core::new(),
            notif: notif,
            spawn_conns: VecDeque::new(),
        }
    }

    /// Spawns an IRC connection for the given user and network.
    pub fn spawn_conn(&mut self, uid: UserId, nid: NetId) {
        self.spawn_conns.push_back((uid, nid));
        self.notif.wakeup().unwrap();
    }

    /// Spawns IRC connections for all users.
    pub fn spawn_conns(&mut self) {
        for (uid, usr) in self.core.iter_users() {
            for (nid, _) in usr.iter_nets() {
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
        let (uid, nid) = seed;
        let addr = if let Some(usr) = scope.core.get_user_mut(&uid) {
            if let Some(net) = usr.get_net_mut(&nid) {
                let result = (net.cfg.server(), net.cfg.port()).to_socket_addrs()
                    .map(|mut iter| iter.next().unwrap());
                match result {
                    Ok(addr) => addr,
                    Err(e) => {
                        error!("Error parsing network address for network {}: {:?}", &nid, e);
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
            Ok(sock) => Stream::new(sock, (uid, nid), scope)
                .map(ConnSpawner::Conn, |_| unreachable!("Connection spawned machine")),
            Err(e) => {
                error!("Error connecting to IRC server for user {} on network {}: {}",
                       uid, nid, e);
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
