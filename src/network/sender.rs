use std::sync::mpsc::{channel, Sender, Receiver};
use rotor::Notifier;
use rotor_irc::Message;

/// Stores connection-specific state information for an IRC network and provides
/// an interface for sending messages to the IRC server.
///
/// This works similarly to `UserClientHandle`, in that the IRC connection state
/// machine will call `register_conn` on the network object to register itself
/// as the connection for that network.
#[derive(Clone)]
pub struct IrcSender {
    tx: Sender<Message>,
    notif: Notifier,
}

impl IrcSender {
    pub fn new(notif: Notifier) -> (IrcSender, IrcSendRx) {
        let (tx, rx) = channel();
        let conn = IrcSender {
            tx: tx,
            notif: notif,
        };
        let rx = IrcSendRx {
            rx: rx,
        };
        (conn, rx)
    }

    pub fn send(self, msg: Message) -> Option<Self> {
        if self.tx.send(msg).is_ok() && self.notif.wakeup().is_ok() {
            Some(self)
        } else {
            None
        }
    }

    pub fn send_all(self, msgs: Vec<Message>) -> Option<Self> {
        for msg in msgs {
            if self.tx.send(msg).is_err() {
                return None;
            }
        }
        if self.notif.wakeup().is_ok() { Some(self) }
        else { None }
    }
}

/// The receiving end of an `IrcSender`.
///
/// The IRC connection state machine should read from this and send the messages
/// to IRC.
pub struct IrcSendRx {
    rx: Receiver<Message>,
}


impl IrcSendRx {
    /// Receives from the paired sender.
    ///
    /// If the sender is dropped, this returns `Err`. Otherwise, returns
    /// `Ok(Some(msg))` if there is a message to send, or `Ok(None)` otherwise.
    pub fn recv(&mut self) -> Result<Option<Message>, ()> {
        use std::sync::mpsc::TryRecvError::*;
        match self.rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(Empty) => Ok(None),
            Err(Disconnected) => Err(()),
        }
    }
}
