//! This module implements the system by which buffers and networks broadcast
//! core messages in response to some event occurring on IRC.

use std::marker::PhantomData;

use common::alert::Alert;

/// An `UpdateHandle` is an object passed in to networks objects and buffers
/// which allows them to send messages to their user's connected clients.
///
/// The type parameter `M` is the type of message the handle sends.
pub trait UpdateHandle<M> : Sized {
    fn send_clients(&mut self, msg: M);
    fn post_alert(&mut self, alert: Alert);


    /// Wraps messages to this handle with the given function.
    fn wrap<'a, F, N>(&'a mut self, func: F) -> WrappedUpdateHandle<'a, F, N, M, Self>
        where F : FnMut(N) -> M
    {
        WrappedUpdateHandle {
            inner: self,
            func: func,
            msgt: PhantomData,
            imsgt: PhantomData,
        }
    }
}


/// A basic update handle that buffers messages and alerts in vectors for
/// sending later.
pub struct BaseUpdateHandle<M> {
    msgs: Vec<M>,
    alerts: Vec<Alert>,
}

impl<M> UpdateHandle<M> for BaseUpdateHandle<M> {
    fn send_clients(&mut self, msg: M) {
        self.msgs.push(msg);
    }

    fn post_alert(&mut self, alert: Alert) {
        debug!("Posting alert {:?}", alert);
        self.alerts.push(alert);
    }
}

impl<M> BaseUpdateHandle<M> {
    pub fn new() -> Self {
        BaseUpdateHandle {
            msgs: vec![],
            alerts: vec![],
        }
    }

    /// Takes the list of alerts.
    pub fn take_alerts(&mut self) -> Vec<Alert> {
        use std::mem;
        let mut alerts = vec![];
        mem::swap(&mut alerts, &mut self.alerts);
        alerts
    }

    /// Takes the list of messages.
    pub fn take_msgs(&mut self) -> Vec<M> {
        use std::mem;
        let mut msgs = vec![];
        mem::swap(&mut msgs, &mut self.msgs);
        msgs
    }
}


/// Wraps another update handle, transforming core messages with a closure.
pub struct WrappedUpdateHandle<'a, F, M, N, I : UpdateHandle<N> + 'a>
    where F : FnMut(M) -> N
{
    inner: &'a mut I,
    func: F,
    msgt: PhantomData<M>,
    imsgt: PhantomData<N>,
}

impl<'a, F, M, N, I : UpdateHandle<N>> UpdateHandle<M> for WrappedUpdateHandle<'a, F, M, N, I>
    where F : FnMut(M) -> N
{
    fn send_clients(&mut self, msg: M) {
        self.inner.send_clients((self.func)(msg));
    }

    fn post_alert(&mut self, alert: Alert) {
        self.inner.post_alert(alert);
    }
}
