#[macro_use] extern crate log;
extern crate irc;
extern crate env_logger;
extern crate rustc_serialize;

use std::collections::HashMap;
use irc::client::prelude::Config;

pub mod config;
pub mod user;
pub mod network;
pub mod buffer;
pub mod line;
pub mod util;
pub mod types;
pub mod conn;

use self::config::{UserConfig, IrcNetConfig};
use self::user::{UserThread};

fn main() {
    env_logger::init().expect("Failed to initialize logger");

    let mut cfg = UserConfig {
        name: "test".to_owned(),
        networks: HashMap::new(),
    };
    cfg.networks.insert("esper".to_owned(), IrcNetConfig(Config {
        nickname: Some("cctest".to_owned()),
        server: Some("irc.esper.net".to_owned()),
        channels: Some(vec!["#Forkk13".to_owned()]),
        .. Config::default()
    }));
    let thr = UserThread::spawn(&cfg);
    thr.join().expect("User thread crashed");
}
