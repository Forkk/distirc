//! Defines the interface for building IRC state machines.

use std::error::Error;
use std::collections::VecDeque;
use std::io::Write;
use rotor::{Scope};
use rotor_stream::{Protocol, Intent, Transport, Exception};
use rotor::mio::tcp::{TcpStream};

use message::{Message};

const MAX_MSG_LEN: usize = 65536;

pub trait IrcMachine : Sized {
    type Context;
    type Seed;

    /// Called when the IRC connection is initially created.
    fn create(seed: Self::Seed, scope: &mut Scope<Self::Context>) -> IrcAction<Self>;

    /// Called when a message is received.
    fn recv(self, msg: Message, scope: &mut Scope<Self::Context>) -> IrcAction<Self>;

    // /// A timeout occurred.
    // fn timeout(self, scope: &mut Scope<Self::Context>) -> IrcAction<Self>;

    /// The machine was woken up.
    fn wakeup(self, scope: &mut Scope<Self::Context>) -> IrcAction<Self>;

    /// Method called when we've disconnected from the server for any reason.
    ///
    /// The state machine must be consumed by this method.
    fn disconnect(self, scope: &mut Scope<Self::Context>);
}


/// An IRC connection state machine abstraction.
pub struct IrcConnection<M : IrcMachine> {
    fsm: M,
    sendq: VecDeque<Message>,
}

impl<M : IrcMachine> IrcConnection<M> {
    /// Calls the given function with the FSM as an arg and handles the
    /// resulting action.
    fn action<F>(mut self, f: F) -> Intent<Self>
        where F : FnOnce(M) -> IrcAction<M>
    {
        let act = f(self.fsm);
        match act.state {
            Ok(fsm) => {
                trace!("Action returned OK");
                self.fsm = fsm;
                for s in act.send { self.sendq.push_back(s); }
                self.idle()
            },
            Err(Some(e)) => {
                error!("Action returned error {}", e);
                Intent::error(e)
            },
            Err(None) => {
                info!("Action returned done. Connection will exit");
                Intent::done()
            },
        }
    }

    /// Waits for flush if there are messages to send, otherwise waits for more
    /// messages from the server.
    fn idle(self) -> Intent<Self> {
        if self.sendq.is_empty() {
            self.wait_for_data()
        } else {
            trace!("There are messages to send. Waiting for output flush.");
            Intent::of(self).expect_flush()
        }
    }

    /// Waits for a new message.
    fn wait_for_data(self) -> Intent<Self> {
        trace!("Waiting for data");
        Intent::of(self).expect_delimiter("\r\n".as_bytes(), MAX_MSG_LEN)
    }

    /// Calls `disconnect` on the state machine and returns the given error.
    fn fail(self, e: Box<Error>) -> Intent<Self> {
        error!("Connection failed. Error: {}", e);
        Intent::error(e)
    }
}


impl<M : IrcMachine> Protocol for IrcConnection<M> {
    type Context = M::Context;
    type Socket = TcpStream;
    type Seed = M::Seed;

    fn create(seed: Self::Seed, _sock: &mut TcpStream, scope: &mut Scope<Self::Context>) -> Intent<Self> {
        debug!("Starting IRC connection");
        let act = M::create(seed, scope);
        match act.state {
            Ok(fsm) => {
                let mut conn = IrcConnection {
                    fsm: fsm,
                    sendq: VecDeque::new(),
                };
                for s in act.send { conn.sendq.push_back(s); }
                conn.idle()
            },
            Err(Some(e)) => {
                error!("Returned error {} from `create()`", e);
                Intent::error(e)
            },
            Err(None) => {
                error!("Returned done from `create()`");
                Intent::done()
            },
        }
    }

    fn bytes_flushed(mut self,
                     transport: &mut Transport<TcpStream>,
                     _scope: &mut Scope<Self::Context>)
                     -> Intent<Self>
    {
        trace!("Message bytes flushed");
        if let Some(msg) = self.sendq.pop_front() {
            let ref mut out = transport.output();
            debug!("Sent message {}", msg);
            match out.write_fmt(format_args!("{}\r\n", msg)) {
                Ok(()) => self.idle(),
                Err(e) => self.fail(Box::new(e) as Box<Error>),
            }
        } else {
            warn!("Waited for flush, but there were no messages to send");
            self.idle()
        }
    }

    fn bytes_read(self,
                  transport: &mut Transport<TcpStream>,
                  end: usize,
                  scope: &mut Scope<Self::Context>)
                  -> Intent<Self>
    {
        let data = transport.input()[0..end].to_vec();
        // As `end` doesn't include the "\r\n" delimiter, we consume an
        // additional two bytes to ensure we don't leave the delimiter in our
        // input stream.
        transport.input().consume(end + 2);
        let line = match String::from_utf8(data) {
            Ok(line) => line,
            Err(e) => return self.fail(Box::new(e) as Box<Error>),
        };
        debug!("Received line: {}", line);
        match line.parse::<Message>() {
            Ok(msg) => self.action(move |m| m.recv(msg, scope)),
            Err(e) => self.fail(Box::new(e) as Box<Error>),
        }
    }

    fn wakeup(self, _t: &mut Transport<TcpStream>, scope: &mut Scope<Self::Context>) -> Intent<Self> {
        debug!("IRC machine woke up");
        self.action(|m| m.wakeup(scope))
    }

    fn exception(self,
                 _t: &mut Transport<Self::Socket>,
                 reason: Exception,
                 scope: &mut Scope<Self::Context>)
                 -> Intent<Self> {
        error!("Error reading data: {}", reason);
        self.fsm.disconnect(scope);
        Intent::error(Box::new(reason) as Box<Error>)
    }

    fn fatal(self, reason: Exception, scope: &mut Scope<Self::Context>) -> Option<Box<Error>> {
        error!("Error reading data: {}", reason);
        self.fsm.disconnect(scope);
        Some(Box::new(reason))
    }

    fn timeout(self, _tp: &mut Transport<TcpStream>, _s: &mut Scope<Self::Context>) -> Intent<Self> {
        // TODO: Implement connection timeouts
        unreachable!()
    }
}




/// An action performed by an IRC machine.
pub struct IrcAction<M> {
    state: Result<M, Option<Box<Error>>>,
    send: Vec<Message>,
}

impl<M> IrcAction<M> {
    /// Constructs an IRC action which will continue running the connection.
    pub fn ok(machine: M) -> IrcAction<M> {
        IrcAction {
            state: Ok(machine),
            send: vec![],
        }
    }

    /// Closes the connection with the given error.
    pub fn error(e: Box<Error>) -> Self {
        IrcAction {
            state: Err(Some(e)),
            send: vec![],
        }
    }

    /// Gracefully closes the connection.
    pub fn close() -> Self {
        IrcAction {
            state: Err(None),
            send: vec![],
        }
    }

    /// Adds the given message to be sent by this action.
    ///
    /// Note that queued messages will not be sent if this action is an error or
    /// close.
    pub fn send(mut self, msg: Message) -> Self {
        self.send.push(msg);
        self
    }

    /// Sends the given vector of messages.
    ///
    /// Note that queued messages will not be sent if this action is an error or
    /// close.
    pub fn send_all(mut self, mut msg: Vec<Message>) -> Self {
        self.send.append(&mut msg);
        self
    }
}
