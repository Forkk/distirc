//! This module implements the client's "alert" system.
//!
//! Alerts are short messages used to let the user know about some notable event
//! such as an error or a ping.

use time;
use time::{Tm, Duration};

use super::TermUi;
use model::BufKey;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertKind {
    // Error,
    // Warning,
    Ping(BufKey),
    Notice,
}

pub type AlertAction = Box<FnMut(&mut TermUi)>;

/// This struct defines information about an alert.
pub struct Alert {
    pub kind: AlertKind,
    pub msg: String,
    /// The duration to show the alert for. If `None`, the alert will be shown
    /// until the user opens it.
    time: Option<Duration>,
    /// Optional action to perform when the alert is opened.
    action: Option<AlertAction>,
}

impl Alert {
    // pub fn error(msg: String) -> Alert { Self::new(msg, AlertKind::Error) }
    // pub fn warning(msg: String) -> Alert { Self::new(msg, AlertKind::Warning) }
    pub fn ping(k: BufKey, msg: String) -> Alert { Self::new(msg, AlertKind::Ping(k)) }
    pub fn notice(msg: String) -> Alert { Self::new(msg, AlertKind::Notice) }

    pub fn new(msg: String, kind: AlertKind) -> Alert {
        Alert {
            kind: kind,
            msg: msg,
            time: None,
            action: None,
        }
    }

    /// Sets an action for this alert.
    ///
    /// Alerts with actions can be "opened" by the user to do things like switch
    /// to an associated buffer.
    pub fn action<F>(mut self, f: F) -> Self
        where F : FnMut(&mut TermUi) + 'static
    {
        let act = Box::new(f) as AlertAction;
        self.action = Some(act);
        self
    }

    /// Sets this alert to disappear after the given duration.
    #[allow(dead_code)]
    pub fn timeout(mut self, time: Duration) -> Self {
        self.time = Some(time);
        self
    }
}



/// Active alert state.
struct AlertState {
    def: Alert,
    shown_at: Tm,
}


/// UI component for storing alerts and showing them on screen.
pub struct AlertList {
    alerts: Vec<AlertState>,
}

impl AlertList {
    pub fn new() -> AlertList {
        AlertList {
            alerts: vec![],
        }
    }

    /// Pushes a new alert into the list.
    pub fn push(&mut self, alert: Alert) {
        let state = AlertState {
            def: alert,
            shown_at: time::now(),
        };
        self.alerts.push(state);
        self.alerts.sort_by_key(|a| a.def.kind.clone());
    }

    /// Updates the alert list, removing any alerts which have exceeded their
    /// duration.
    pub fn update(&mut self) {
        let now = time::now();
        self.alerts.retain(|s| {
            if let Some(t) = s.def.time {
                t > (s.shown_at - now)
            } else { true }
        });
        self.alerts.sort_by_key(|a| a.def.kind.clone());
    }

    pub fn count(&self) -> usize {
        self.alerts.len()
    }

    pub fn get(&self, i: usize) -> &Alert {
        &self.alerts[i].def
    }

    pub fn iter<'a>(&'a self) -> Box<Iterator<Item=&Alert> + 'a> {
        Box::new(self.alerts.iter().map(|a| &a.def)) as Box<Iterator<Item=_>>
    }

    /// Dismisses the given alert and returns its action if present.
    ///
    /// If the alert has no associated action, this does nothing.
    pub fn activate(&mut self, i: usize) -> Option<AlertAction> {
        if self.alerts[i].def.action.is_some() {
            let a = self.alerts.remove(i);
            a.def.action
        } else {
            None
        }
    }
}
