#[macro_use] extern crate log;
#[macro_use] extern crate rotor;
extern crate rotor_stream;
extern crate irc;
extern crate env_logger;
extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
extern crate time;
extern crate toml;

extern crate common;

use std::path::Path;
use rotor::{Machine, Response, Loop, Config as LoopCfg};
use rotor::mio::tcp::TcpListener;
use rotor_stream::Accept;

use common::conn::ConnStream;

pub mod config;
pub mod user;
pub mod handle;
pub mod network;
pub mod buffer;
pub mod conn;

use self::config::read_config;
use self::conn::{Client, Context, Updater};

rotor_compose!{
    pub enum Fsm/Seed<Context> {
        Client(Accept<ConnStream<Client>, TcpListener>),
        Updater(Updater),
    }
}

fn main() {
    env_logger::init().expect("Failed to initialize logger");

    let cfg_path = Path::new("config.toml");
    let cfg = read_config(cfg_path);

    debug!("Creating loop.");
    let mut loop_creator = Loop::new(&LoopCfg::new()).unwrap();
    let sock = TcpListener::bind(&"127.0.0.1:4242".parse().unwrap()).unwrap();
    loop_creator.add_machine_with(|scope| {
        Accept::<ConnStream<Client>, _>::new(sock, (), scope).wrap(Fsm::Client)
    }).unwrap();

    let mut notif = None;
    loop_creator.add_machine_with(|scope| {
        notif = Some(scope.notifier());
        Response::ok(Updater).wrap(Fsm::Updater)
    }).expect("Failed to add updater");
    let notif = notif.expect("Notifier was not set.");

    debug!("Creating context.");
    let mut ctx = Context::new(notif);
    for (uid, ucfg) in cfg.users.iter() {
        ctx.add_user(uid, ucfg.clone());
    }

    debug!("Initializing context.");
    ctx.init();

    debug!("Starting");
    loop_creator.run(ctx).unwrap();
}
