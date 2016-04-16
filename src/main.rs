#[macro_use] extern crate log;
#[macro_use] extern crate rotor;
extern crate rotor_stream;
extern crate irc;
extern crate env_logger;
extern crate rustc_serialize;
extern crate time;

extern crate common;

use std::collections::HashMap;
use rotor::{Loop, Config as LoopCfg};
use rotor::mio::tcp::TcpListener;
use rotor_stream::Accept;
use irc::client::prelude::Config as IrcConfig;

use common::conn::ConnStream;

pub mod config;
pub mod user;
pub mod network;
pub mod buffer;
pub mod core;

use self::config::{UserConfig, IrcNetConfig};
use self::core::{Client, Context};
// use self::user::{UserThread};

fn main() {
    env_logger::init().expect("Failed to initialize logger");

    let mut cfg = UserConfig {
        name: "test".to_owned(),
        networks: HashMap::new(),
    };
    cfg.networks.insert("esper".to_owned(), IrcNetConfig {
        name: "esper".to_owned(),
        cfg: IrcConfig {
            nickname: Some("cctest".to_owned()),
            server: Some("irc.esper.net".to_owned()),
            channels: Some(vec!["#Forkk13".to_owned()]),
            .. IrcConfig::default()
        }
    });
    debug!("Created test config.");

    debug!("Creating context.");
    let mut ctx = Context::new();
    ctx.add_user("Forkk", cfg);

    debug!("Initializing context.");
    ctx.init();

    debug!("Creating loop.");
    let mut loop_creator = Loop::new(&LoopCfg::new()).unwrap();
    let sock = TcpListener::bind(&"127.0.0.1:4242".parse().unwrap()).unwrap();
    loop_creator.add_machine_with(|scope| {
        Accept::<ConnStream<Client>, _>::new(sock, (), scope)
    }).unwrap();

    debug!("Starting");
    loop_creator.run(ctx).unwrap();
}
