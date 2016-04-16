#[macro_use] extern crate log;
#[macro_use] extern crate rotor;
extern crate env_logger;
extern crate rotor_stream;
extern crate rustbox;
extern crate time;

extern crate common;

use std::net::SocketAddr;

pub mod ui;
pub mod model;
pub mod conn;

use self::ui::TermUi;
use self::conn::ConnThread;

fn main() {
    env_logger::init().expect("Failed to initialize logger");

    let addr = "127.0.0.1:4242".parse::<SocketAddr>().unwrap();
    let conn = ConnThread::spawn(addr);

    let mut ui = TermUi::new().expect("Failed to initialize UI");
    ui.main(conn);
}
