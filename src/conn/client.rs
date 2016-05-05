use std::collections::HashMap;
use rotor::Scope;

use common::conn::{Handler, Action};
use common::messages::{
    BufTarget, CoreMsg, CoreNetMsg, CoreBufMsg,
    ClientMsg, ClientNetMsg, ClientBufMsg,
};

use state::{UserHandle, UserClientHandle};
use config::UserId;
use network::IrcNetwork;
use handle::{UpdateHandle, BaseUpdateHandle};

use super::Context;


/// This machine handles a client's state.
pub enum Client {
    /// The client has just connected and hasn't authenticated yet.
    Authing,
    /// The client has authenticated as a user.
    Connected {
        uid: UserId,
        rx: UserClientHandle,
        bufs: HashMap<BufTarget, ClientBuf>,
    },
}

/// Stores information about what we've already sent the client.
pub struct ClientBuf {
    /// The index of the last scrollback message we sent.
    last_sent_idx: isize,
}

impl Client {
    fn handle_auth_msgs(msg: &ClientMsg, s: &mut Scope<Context>) -> Action<Self> {
        if let &ClientMsg::Authenticate(ref uid, ref pass) = msg {
            let notif = s.notifier();
            let usr = match s.core.get_user_mut(uid) {
                Some(u) => u,
                None => {
                    error!("Unknown user state: {}", uid);
                    return Action::done();
                },
            };
            if &usr.cfg.password == &pass.0 {
                info!("Client authenticated successfully as {}", uid);

                // Register our client with the user.
                let rx = usr.register_client(notif);

                // Send the networks list.
                let mut nets = vec![];
                for (_nid, net) in usr.iter_nets() {
                    nets.push(net.to_info());
                }

                let me = Client::Connected {
                    uid: uid.to_owned(),
                    rx: rx,
                    bufs: HashMap::new(),
                };
                Action::ok(me)
                    .send(CoreMsg::AuthOk)
                    .send(CoreMsg::Networks(nets))
            } else {
                Action::ok(Client::Authing).send(CoreMsg::AuthErr)
            }
        } else {
            error!("Client failed to send authentication request during auth phase. Aborting connection");
            Action::done()
        }
    }
}


impl Handler for Client {
    type Context = Context;
    type Seed = ();
    type Send = CoreMsg;
    type Recv = ClientMsg;

    fn create(_seed: (), _s: &mut Scope<Self::Context>) -> Action<Self> {
        info!("New client connected. Awaiting authentication.");
        Action::ok(Client::Authing)
    }

    /// A message has been received.
    fn msg_recv(self, msg: &Self::Recv, s: &mut Scope<Self::Context>) -> Action<Self> {
        info!("Received message: {:?}", msg);
        match self {
            Client::Authing => {
                Self::handle_auth_msgs(msg, s)
            },
            Client::Connected { uid, rx, bufs } => {
                let mut user = match s.core.get_user_mut(&uid) {
                    Some(u) => u,
                    None => {
                        error!("Unknown user state: {}", uid);
                        return Action::done();
                    },
                };
                Client::Connected { uid: uid, rx: rx, bufs: bufs }.handle_user_msg(msg, &mut user)
            },
        }
    }

    /// A timeout occurred.
    fn timeout(self, _scope: &mut Scope<Self::Context>) -> Action<Self> {
        unreachable!("Unexpected timeout")
    }

    fn wakeup(self, _s: &mut Scope<Self::Context>) -> Action<Self> {
        trace!("Client woke up");
        match self {
            Client::Authing => {
                warn!("Client was woken up during authentication phase");
                Action::ok(self)
            },
            Client::Connected { uid, mut rx, bufs } => {
                // Send new messages to the client.
                let mut msgs = vec![];
                while let Some(msg) = rx.recv() {
                    // FIXME: This hack doesn't work. We need to find another way.
                    // // This is hacky, but it's really the only way to catch when
                    // // a client is told about a buffer. We do this so we can
                    // // ensure that we set a buffer's last sent index to the
                    // // appropriate line.
                    // if let CoreMsg::NetMsg(ref nid, CoreNetMsg::Buffers(ref bs)) = msg {
                    //     for buf in bs {
                    //         bufs.insert(buf.id.clone(), ClientBuf {
                    //             last_sent_idx: s.core.get_user(&uid).unwrap()
                    //                 .get_net(&nid).unwrap()
                    //                 .get_buf(&buf.id).unwrap()
                    //                 .front_len(),
                    //         });
                    //     }
                    // }
                    trace!("Sending client message: {:?}", msg);
                    msgs.push(msg);
                }
                let mut a = Action::ok(Client::Connected{ uid: uid, rx: rx, bufs: bufs });
                a = a.send_all(msgs.clone());
                a
            }
        }
    }
}

impl Client {
    fn handle_user_msg(self, msg: &ClientMsg, user: &mut UserHandle) -> Action<Self> {
        let mut uh = BaseUpdateHandle::<CoreMsg>::new();
        let act = match *msg {
            ClientMsg::NetMsg(ref nid, ref msg) => {
                if let Some(ref mut net) = user.get_net_mut(&nid) {
                    self.handle_net_msg(msg, net, &mut uh)
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
                for (_nid, net) in user.iter_nets() {
                    nets.push(net.to_info());
                }
                Action::ok(self).send(CoreMsg::Networks(nets))
            },
            ClientMsg::Authenticate(_, _) => {
                error!("Authenticated client sent auth request. Ignoring.");
                Action::ok(self)
            }
        };
        user.exec_update_handle(uh);
        act
    }

    fn handle_net_msg(self,
                      msg: &ClientNetMsg,
                      net: &mut IrcNetwork,
                      u: &mut BaseUpdateHandle<CoreMsg>)
                      -> Action<Self>
    {
        let nid = net.id().clone();
        let mut u = u.wrap(|msg| CoreMsg::NetMsg(nid.clone(), msg));
        match *msg {
            ClientNetMsg::BufMsg(ref targ, ref msg) => {
                if let Some(_) = net.get_buf(&targ) {
                    self.handle_buf_msg(msg, targ, net, &mut u)
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
                if let Err(e) = net.send_join_chan(chan.clone(), &mut u) {
                    Action::ok(self).send(CoreMsg::Status(format!("Can't join channel: {}", e)))
                } else {
                    Action::ok(self)
                }
            },
            ClientNetMsg::PartChan(ref chan, ref optmsg) => {
                if let Err(e) = net.send_part_chan(chan.clone(), optmsg.clone(), &mut u) {
                    Action::ok(self).send(CoreMsg::Status(format!("Can't part channel: {}", e)))
                } else {
                    Action::ok(self)
                }
            },
            ClientNetMsg::ChangeNick(ref nick) => {
                if let Err(e) = net.send_change_nick(nick.clone(), &mut u) {
                    Action::ok(self).send(CoreMsg::Status(format!("Can't change nick: {}", e)))
                } else {
                    Action::ok(self)
                }
            },
        }
    }

    fn handle_buf_msg<U>(self,
                         msg: &ClientBufMsg,
                         targ: &BufTarget,
                         net: &mut IrcNetwork,
                         u: &mut U)
                         -> Action<Self>
        where U : UpdateHandle<CoreNetMsg>
    {
        match *msg {
            ClientBufMsg::SendMsg(ref msg, ref kind) => {
                if let Err(e) = net.send_chat_msg(targ.clone(), msg.clone(), kind.clone(), u) {
                    Action::ok(self).send(CoreMsg::Status(format!("Can't send to channel: {}", e)))
                } else {
                    Action::ok(self)
                }
            },
            ClientBufMsg::FetchLogs(count) => {
                let buf = net.get_buf_mut(targ).unwrap();

                let (mut bufs, rx, uid) = if let Client::Connected { bufs, rx, uid } = self {
                    (bufs, rx, uid)
                } else { unreachable!(); };

                let lines = {
                    let mut cb = bufs.entry(targ.clone()).or_insert_with(|| {
                        error!("Missing `ClientBuf` entry for {:?}. Scrollback will probably be sent incorrectly.",
                               targ);
                        ClientBuf {
                            last_sent_idx: buf.front_len(),
                        }
                    });
                    let start = cb.last_sent_idx - 1;
                    let mut lines = vec![];
                    for i in 0..count as isize {
                        if let Some(line) = buf.get_line(start - i) {
                            lines.push(line.clone());
                            cb.last_sent_idx -= 1;
                        } else {
                            break;
                        }
                    }
                    lines
                };
                let nmsg = CoreNetMsg::BufMsg(buf.id().clone(), CoreBufMsg::Scrollback(lines));
                Action::ok(Client::Connected {
                    bufs: bufs, rx: rx, uid: uid
                }).send(CoreMsg::NetMsg(buf.nid().clone(), nmsg))
            },
        }
    }
}
