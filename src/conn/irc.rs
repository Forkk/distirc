//! Manages a network's IRC connection

use rotor::Scope;
use rotor_irc::{Message, Command, IrcMachine, IrcAction};

use common::types::NetId;
use common::messages::CoreMsg;

use conn::Context;
use config::UserId;
use handle::{UpdateHandle, BaseUpdateHandle};
use network::IrcSendRx;

/// Gets a user from the scope or closes the connection.
macro_rules! try_usr {
    // ( $scope: expr, $uid: expr ) => {
    //     try_usr!(target: module_path!(), $scope, $uid)
    // };
    ( $logid: expr, $scope: expr, $uid: expr ) => {
        if let Some(usr) = $scope.core.get_user_mut($uid) {
            usr
        } else {
            error!("{}: Missing associated user {} for IRC network connection", $logid, $uid);
            return IrcAction::close();
        }
    };
}

/// Gets a network from the scope or closes the connection.
macro_rules! try_net {
    // ( $scope: expr, $uid: expr ) => {
    //     try_net!(target: module_path!(), $scope, $uid)
    // };
    ( $logid: expr, $usr: expr, $nid: expr ) => {{
        if let Some(net) = $usr.get_net_mut($nid) {
            net
        } else {
            error!("{}: Missing associated network object {} for IRC network connection", $logid, $nid);
            // TODO: Make an error type for this.
            return IrcAction::close();
        }
    }};
}


/// State machine for IRC network connections.
pub struct IrcNetConn {
    uid: UserId,
    nid: NetId,
    rx: IrcSendRx,
    state: NetConnState,
    // Identification string printed in log messages.
    log_id: String,
}

/// This enum represents the connection's various states of inititialization.
///
/// If, for example, the state is `Identifying`, the connection state machine
/// will wait for `RPL_MYINFO`, authenticate with `NickServ`, and go into the
/// `Authing` state.
enum NetConnState {
    /// Waiting for the server to respond to our `USER` and `NICK` messages.
    /// This waits for `RPL_MYINFO` and then auths with `NickServ` if
    /// applicable.
    Identifying,
    /// We're waiting on `NickServ` to respond to our authentication. If this
    /// fails, we log an error and move on to `Joining` anyay.
    Authing,
    // /// We're waiting on joining initial channels.
    // Joining,
    Connected,
}

impl IrcMachine for IrcNetConn {
    type Context = Context;
    type Seed = (UserId, NetId);

    fn create(seed: (UserId, NetId), scope: &mut Scope<Self::Context>) -> IrcAction<Self> {
        let (uid, nid) = seed;
        let log_id = format!("{}.{}", uid, nid);
        debug!("{}: Starting IRC connection", &log_id);

        let notif = scope.notifier();
        let usr = try_usr!(&log_id, scope, &uid);
        let mut u = BaseUpdateHandle::<CoreMsg>::new();
        let (rx, nname, uname, rname) = {
            let mut net = try_net!(&log_id, usr, &nid);
            let rx = net.register_conn(notif, &mut u);
            (rx, net.cfg.nick().to_owned(), net.cfg.username().to_owned(), net.cfg.realname().to_owned())
        };
        usr.exec_update_handle(u);

        let state = IrcNetConn {
            uid: uid,
            nid: nid,
            rx: rx,
            state: NetConnState::Identifying,
            log_id: log_id,
        };
        info!("{}: Started IRC connection", &state.log_id);
        IrcAction::ok(state)
            .send(Message {
                prefix: None,
                command: Command::USER,
                args: vec![uname, "0".to_owned(), "*".to_owned()],
                body: Some(rname),
            })
            .send(Message {
                prefix: None,
                command: Command::NICK,
                args: vec![nname],
                body: None,
            })
    }

    fn recv(mut self, msg: Message, scope: &mut Scope<Self::Context>) -> IrcAction<Self> {
        debug!("{}: Received message: {}", &self.log_id, msg);
        let usr = try_usr!(&self.log_id, scope, &self.uid);
        let mut msgs = vec![];
        let mut u = BaseUpdateHandle::<CoreMsg>::new();

        if let Message { command: Command::PING, args, body, .. } = msg {
            debug!("Sending pong: {:?} {:?}", args, body);
            msgs.push(Message {
                prefix: None,
                command: Command::PONG,
                args: args,
                body: body,
            });
        } else {
            use rotor_irc::Response::*;
            let mut net = try_net!(&self.log_id, usr, &self.nid);
            let nid = self.nid.clone();

            // TODO: Implement SASL authentication
            match self.state {
                NetConnState::Identifying => {
                    net.handle_msg(msg.clone(), &mut u.wrap(|msg| CoreMsg::NetMsg(nid.clone(), msg)));
                    if let Message { command: Command::Response(RPL_WELCOME), .. } = msg {
                        if let Some(pass) = net.cfg.nickserv_pass() {
                            info!("{}: Authenticating with NickServ", &self.log_id);
                            msgs.push(Message {
                                prefix: None,
                                command: Command::PRIVMSG,
                                args: vec!["NickServ".to_owned()], // TODO: Allow configuring `NickServ`'s nick
                                body: Some(format!("identify {}", pass)),
                            });
                            self.state = NetConnState::Authing;
                        } else {
                            info!("{}: No NickServ auth. Joining channels", &self.log_id);
                            // If we don't have a `NickServ` password, skip straight
                            // to joining channels.
                            msgs.push(Message {
                                prefix: None,
                                command: Command::JOIN,
                                args: vec![net.cfg.channels().join(",")],
                                body: None,
                            });
                            self.state = NetConnState::Connected;
                        }
                    }
                },
                NetConnState::Authing => {
                    net.handle_msg(msg.clone(), &mut u.wrap(|msg| CoreMsg::NetMsg(nid.clone(), msg)));
                    // FIXME: Maybe we should do something more fancy than just
                    // waiting for any NOTICE from NickServ.
                    if let Message { command: Command::NOTICE, body: Some(body), .. } = msg {
                        info!("{}: NickServ authentication finished. Reply: {}", &self.log_id, body);
                        msgs.push(Message {
                            prefix: None,
                            command: Command::JOIN,
                            args: vec![net.cfg.channels().join(",")],
                            body: None,
                        });
                        self.state = NetConnState::Connected;
                    }
                }
                NetConnState::Connected => {
                    trace!("{}: Handling message as connected", &self.log_id);
                    net.handle_msg(msg, &mut u.wrap(|msg| CoreMsg::NetMsg(nid.clone(), msg)));
                },
            }
        }
        usr.exec_update_handle(u);
        for msg in msgs.iter() {
            debug!("{}: Sending message: {}", &self.log_id, msg);
        }
        IrcAction::ok(self).send_all(msgs)
    }

    fn wakeup(mut self, _s: &mut Scope<Self::Context>) -> IrcAction<Self> {
        let mut msgs = vec![];
        loop {
            match self.rx.recv() {
                Ok(Some(msg)) => {
                    debug!("{}: Sending message: {}", &self.log_id, msg);
                    msgs.push(msg);
                },
                Ok(None) => break,
                Err(_) => {
                    error!("{}: IRC message sender dropped. Disconnecting", &self.log_id);
                    return IrcAction::close();
                },
            }
        }
        trace!("{}: Sending messages: {:?}", &self.log_id, msgs);
        IrcAction::ok(self).send_all(msgs)
    }

    fn disconnect(self, scope: &mut Scope<Self::Context>) {
        info!("{}: Disconnected from IRC", &self.log_id);
        if let Some(usr) = scope.core.get_user_mut(&self.uid) {
            let mut u = BaseUpdateHandle::<CoreMsg>::new();
            if let Some(net) = usr.get_net_mut(&self.nid) {
                let nid = self.nid.clone();
                net.disconnect(&mut u.wrap(|msg| CoreMsg::NetMsg(nid.clone(), msg)));
            } else {
                error!("{}: Missing associated network {} for IRC network connection", &self.log_id, self.nid);
                return;
            }
            usr.exec_update_handle(u);
        } else {
            error!("{}: Missing associated user {} for IRC network connection", &self.log_id, self.uid);
            return;
        }
    }
}
