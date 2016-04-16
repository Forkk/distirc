use std::error::Error;
use std::thread;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::net::SocketAddr;
use rotor::{Scope, Loop, Config as LoopCfg, Notifier};
use rotor_stream::{Persistent, Stream};

use common::conn::{Action, Handler, Connection};
use common::messages::{CoreMsg, ClientMsg};


/// Handle for communicating with the connection thread.
pub struct ConnThread {
    rx: Receiver<CoreMsg>,
    tx: Sender<ClientMsg>,
    /// Notifier to wake up the connection machine when we have messages to
    /// send.
    notif: Notifier,
}

impl ConnThread {
    /// Spawns a connection to the given address.
    pub fn spawn(addr: SocketAddr) -> ConnThread {
        // sender/receiver for messages to the server
        let (txs, txr) = channel();
        // sender/receiver for messages from the server
        let (rxs, rxr) = channel();

        let ctx = ConnCtx {
            rxs: rxs,
            txr: txr,
        };
        let mut notif = None;
        let mut mkloop = Loop::new(&LoopCfg::new()).unwrap();
        mkloop.add_machine_with(|scope| {
            notif = Some(scope.notifier());
            Persistent::<Connection<ConnHandler>>::connect(scope, addr, ())
        }).expect("Failed to add connection state machine");

        thread::Builder::new()
            .name("connection".to_owned())
            .spawn(move || mkloop.run(ctx).unwrap())
            .expect("Failed to spawn connection thread");

        ConnThread {
            rx: rxr,
            tx: txs,
            notif: notif.expect("Notifier was not set."),
        }
    }

    /// Sends a message to the server.
    pub fn send(&mut self, msg: ClientMsg) {
        self.tx.send(msg).expect("Failed to send message to connection thread");
        self.notif.wakeup().expect("Failed to wake up connection thread");
    }

    pub fn recv(&mut self) -> Option<CoreMsg> {
        self.rx.try_recv().ok()
    }
}


/// Context object for the connection.
struct ConnCtx {
    rxs: Sender<CoreMsg>,
    txr: Receiver<ClientMsg>,
}

struct ConnHandler;

impl ConnHandler {
    fn send_messages(self, scope: &mut Scope<ConnCtx>) -> Action<Self> {
        debug!("Sending new messages");
        let mut act = Action::ok(self);
        'recv: loop {
            match scope.txr.try_recv() {
                Ok(msg) => act = act.send(msg),
                Err(TryRecvError::Empty) => break 'recv,
                Err(_) => {
                    error!("Outbound message channel closed");
                    return Action::done() // TODO: Implement Action::error()
                },
            }
        }
        act
    }
}

impl Handler for ConnHandler {
    type Context = ConnCtx;
    type Send = ClientMsg;
    type Recv = CoreMsg;

    fn create(_scope: &mut Scope<Self::Context>) -> Action<Self> {
        info!("Created connection handler");
        Action::ok(ConnHandler)
    }

    fn msg_recv(self, msg: &Self::Recv, scope: &mut Scope<Self::Context>) -> Action<Self> {
        scope.rxs.send(msg.clone()).unwrap();
        Action::ok(self)
    }

    fn timeout(self, _scope: &mut Scope<Self::Context>) -> Action<Self> {
        unreachable!("Unexpected connection handler timeout")
    }

    fn wakeup(self, scope: &mut Scope<Self::Context>) -> Action<Self> {
        // On wakeup, check for any messages to send and send them.
        self.send_messages(scope)
    }
}
