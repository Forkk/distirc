//! This module implements state machine boilerplate for sending and receiving
//! encodable messages.

use std::mem;
use std::collections::VecDeque;
use std::error::Error;
use rotor::Scope;
use rotor::mio::tcp::TcpStream;
use rotor_stream::{Stream, Transport, Protocol, Intent, Exception};
use serde::{Serialize, Deserialize};
use bincode::SizeLimit;
use bincode::serde::{serialize_into, serialized_size, deserialize};
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};


pub type ConnStream<H> = Stream<Connection<H>>;

/// Trait for state machines that handle distirc messages.
pub trait Handler: Sized {
    type Context;
    type Send: Sized + Serialize;
    type Recv: Sized + Deserialize;

    fn create(scope: &mut Scope<Self::Context>) -> Action<Self>;

    /// A message has been received.
    fn msg_recv(self, msg: &Self::Recv, scope: &mut Scope<Self::Context>) -> Action<Self>;

    /// A timeout occurred.
    fn timeout(self, scope: &mut Scope<Self::Context>) -> Action<Self>;

    fn wakeup(self, scope: &mut Scope<Self::Context>) -> Action<Self>;
}


/// Encapsulates a state machine and a set of actions.
pub struct Action<M: Handler> {
    machine: Result<M, Option<Box<Error>>>,
    send: Vec<<M as Handler>::Send>,
}

impl<M: Handler> Action<M> {
    pub fn ok(machine: M) -> Action<M> {
        Action {
            machine: Ok(machine),
            send: vec![],
        }
    }

    /// Adds a message to be sent as part of this action.
    pub fn send(mut self, msg: <M as Handler>::Send) -> Action<M> {
        self.send.push(msg);
        self
    }

    /// Adds the given vector of messages to be sent.
    pub fn send_all(mut self, mut msgs: Vec<<M as Handler>::Send>) -> Action<M> {
        self.send.append(&mut msgs);
        self
    }

    pub fn done() -> Action<M> {
        Action {
            machine: Err(None),
            send: vec![],
        }
    }
}


/// The main connection state machine abstraction.
pub struct Connection<H : Handler> {
    fsm: H,
    msgq: VecDeque<<H as Handler>::Send>,
    state: ConnState,
}

enum ConnState {
    /// Waiting for the next message.
    Waiting,
    /// Just read the header for the next message and waiting for the message.
    Reading,
}

impl<H : Handler> Connection<H> {
    /// Executes the given action and returns an `Intent`.
    fn action<F>(mut self, mut f: F) -> Intent<Self>
        where F : FnMut(H) -> Action<H>
    {
        let act = f(self.fsm);
        match act.machine {
            Ok(fsm) => {
                self.fsm = fsm;
                if act.send.is_empty() {
                    self.wait_for_data()
                } else {
                    for msg in act.send {
                        self.msgq.push_back(msg);
                    }
                    Intent::of(self).expect_flush()
                }
            },
            Err(Some(e)) => Intent::error(e),
            Err(None) => Intent::done(),
        }
    }

    /// Waits for a message header.
    fn wait_for_data(mut self) -> Intent<Self> {
        self.state = ConnState::Waiting;
        Intent::of(self).expect_bytes(mem::size_of::<u64>())
    }
}

impl<H : Handler> Protocol for Connection<H> {
    type Context = <H as Handler>::Context;
    type Socket = TcpStream;
    type Seed = ();

    fn create(_seed: (), _sock: &mut TcpStream, scope: &mut Scope<Self::Context>) -> Intent<Self> {
        let act = H::create(scope);
        match act.machine {
            Ok(fsm) => {
                let mut conn = Connection {
                    fsm: fsm,
                    msgq: VecDeque::new(),
                    state: ConnState::Waiting,
                };
                for s in act.send { conn.msgq.push_back(s); }
                Intent::of(conn).expect_flush()
            },
            Err(Some(e)) => Intent::error(e),
            Err(None) => Intent::done(),
        }
    }

    fn bytes_flushed(mut self,
                     transport: &mut Transport<TcpStream>,
                     _scope: &mut Scope<Self::Context>)
                     -> Intent<Self> {
        debug!("Message bytes flushed");
        if let Some(msg) = self.msgq.pop_front() {
            let ref mut out = transport.output();
            if let Err(e) = out.write_u64::<LittleEndian>(serialized_size(&msg) as u64) {
                return Intent::error(Box::new(e) as Box<Error>);
            }
            match serialize_into(out, &msg, SizeLimit::Bounded(65535)) {
                Ok(()) => Intent::of(self).expect_flush(),
                Err(e) => Intent::error(Box::new(e) as Box<Error>),
            }
        } else {
            self.wait_for_data()
        }
    }

    fn bytes_read(mut self,
                  transport: &mut Transport<TcpStream>,
                  end: usize,
                  scope: &mut Scope<Self::Context>)
                  -> Intent<Self> {
        match self.state {
            ConnState::Waiting => {
                let r = {
                    let sz = mem::size_of::<u64>();
                    let mut data = &transport.input()[0..end];
                    debug_assert!(data.len() == sz, "Expected {} byte message size, but size = {}", sz, data.len());
                    data.read_u64::<LittleEndian>()
                };
                transport.input().consume(end);
                match r {
                    Ok(size) => {
                        self.state = ConnState::Reading;
                        Intent::of(self).expect_bytes(size as usize)
                    },
                    Err(e) => {
                        error!("Error reading message size: {}", e);
                        Intent::error(Box::new(e) as Box<Error>)
                    },
                }
            },
            ConnState::Reading => {
                let msg = {
                    let data = &transport.input()[..end];
                    deserialize(data)
                };
                transport.input().consume(end);
                match msg {
                    Ok(msg) => {
                        self.state = ConnState::Waiting;
                        self.action(|f| f.msg_recv(&msg, scope))
                    },
                    Err(e) => {
                        error!("Error reading message: {}", e);
                        Intent::error(Box::new(e) as Box<Error>)
                    },
                }
            }
        }
    }

    fn timeout(self,
               _transport: &mut Transport<TcpStream>,
               scope: &mut Scope<Self::Context>)
               -> Intent<Self> {
        self.action(|f| f.timeout(scope))
    }

    /// Message received (from the main loop)
    fn wakeup(self,
              _transport: &mut Transport<TcpStream>,
              scope: &mut Scope<Self::Context>)
              -> Intent<Self> {
        self.action(|f| f.wakeup(scope))
    }

    fn exception(self,
                 _transport: &mut Transport<Self::Socket>,
                 reason: Exception,
                 _scope: &mut Scope<Self::Context>)
                 -> Intent<Self> {
        error!("Error reading data: {}", reason);
        Intent::done()
    }
    fn fatal(self, reason: Exception, _scope: &mut Scope<Self::Context>) -> Option<Box<Error>> {
        error!("Fatal error reading data: {}", reason);
        None
    }
}
