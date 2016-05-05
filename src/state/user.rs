use std::sync::mpsc::{channel, Sender, Receiver};
use std::ops::{Deref, DerefMut};
use rotor::Notifier;

use common::messages::CoreMsg;
use common::alert::Alert;

use user::User;
use handle::BaseUpdateHandle;


/// A wrapper around a `User` which keeps track of the user's connected clients
/// and provides methods for talking to them.
///
/// This derefs to `User` for convenience.
pub struct UserHandle {
    user: User,
    clients: Vec<UserClient>,
    alerts: Vec<Alert>,
}


impl UserHandle {
    /// Wraps the given user in a new `UserHandle`.
    pub fn new(user: User) -> UserHandle {
        UserHandle {
            user: user,
            clients: vec![],
            alerts: vec![],
        }
    }

    /// Consumes an update handle, sending its messages and alerts to this
    /// user's clients.
    ///
    /// Right now, this function also sends any alerts that have been posted to
    /// the user, not just those posted to the update handle.
    pub fn exec_update_handle(&mut self, mut u: BaseUpdateHandle<CoreMsg>) {
        for msg in u.take_msgs() {
            self.broadcast(&msg);
        }

        if !self.clients.is_empty() {
            // If there are clients connected, send them the alerts.
            let alerts = self.take_alerts();
            self.broadcast(&CoreMsg::Alerts(alerts));
        } else if let Some(ref cmd) = self.cfg.alert_cmd.clone() {
            // Otherwise, run our alert command if there is one.
            use std::process::Command;
            for alert in self.take_alerts() {
                let cmd = cmd.replace("%m", &alert.msg);
                info!("Sending alert with command {}", cmd);
                Command::new("/bin/sh").arg("-c").arg(cmd).spawn().expect("Failed to spawn alert command");
            }
        } else {
            // If all else fails, store the alerts for sending later.
            let mut alerts = self.take_alerts();
            self.alerts.append(&mut alerts);
        }
    }

    /// Takes a vector of alerts that occurred since the last call to this
    /// function.
    fn take_alerts(&mut self) -> Vec<Alert> {
        use std::mem;
        let mut alerts = vec![];
        mem::swap(&mut alerts, &mut self.alerts);
        alerts
    }

    /// Broadcasts the given message to all of this user's clients.
    ///
    /// As a side-effect, this function will also prune any disconnected clients
    /// (clients whose `Receiver`) has been `drop`ed.
    pub fn broadcast(&mut self, msg: &CoreMsg) {
        self.clients.retain(|client| {
            if let Err(_) = client.tx.send(msg.clone()) {
                return false;
            }
            if let Err(_) = client.notif.wakeup() {
                return false;
            }
            true
        });
    }


    /// This function is used to register a client's interest in this user's
    /// client messages.
    ///
    /// When a client authenticates, its connection state machine creates a
    /// notifier that will wake it up, and then calls this method, passing the
    /// notifier to it. When a message is broadcast to the user's clients, the
    /// user will wakeup the notifier and the client will be able to read the
    /// messages from the `UserClientHandle` returned by this function.
    pub fn register_client(&mut self, notif: Notifier) -> UserClientHandle {
        let (tx, rx) = channel();
        let client = UserClient {
            notif: notif,
            tx: tx,
        };
        let handle = UserClientHandle {
            rx: rx,
        };
        self.clients.push(client);
        handle
    }
}

impl Deref for UserHandle {
    type Target = User;
    fn deref(&self) -> &Self::Target { &self.user }
}
impl DerefMut for UserHandle {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.user }
}



/// The sending component for a `UserClientHandle`.
struct UserClient {
    notif: Notifier,
    // TODO: Maybe use some sort of broadcast channel for this instead of
    // individual channels.
    tx: Sender<CoreMsg>,
}

/// Handle for clients to receive messages broadcast to a user's clients.
///
/// These are constructed by calling `UserHandle::register_client` with a
/// `Notifier`. The function will return a `UserClientHandle` which can be used
/// to receive broadcast messages when the notifier is woken up.
pub struct UserClientHandle {
    rx: Receiver<CoreMsg>,
}

impl UserClientHandle {
    /// Gets the next message to send to the core.
    ///
    /// If there are no new messages to send, this returns `None`. Note that it
    /// is conceivable that a handle's associated user may no longer exist. In
    /// this case, we simply return `None`.
    pub fn recv(&mut self) -> Option<CoreMsg> {
        match self.rx.try_recv() {
            Ok(msg) => Some(msg),
            // FIXME: How should we handle the other end disappearing?
            Err(_) => None,
        }
    }
}
