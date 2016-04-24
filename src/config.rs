use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::collections::HashMap;
use std::default::Default;
use toml;
use toml::Parser;
use irc::client::prelude::Config;
use rustc_serialize::Decodable;

use common::types::NetId;

pub type UserId = String;


/// Loads the config file at the given path. Panics on failure.
pub fn read_config(path: &Path) -> ChatConfig {
    info!("Reading config file from {}", path.display());
    let mut s = String::new();

    let mut f = File::open(path).expect("Failed to open config file");
    f.read_to_string(&mut s).expect("Failed to read config file");
    debug!("Read config");

    let mut parser = Parser::new(&s);
    if let Some(table) = parser.parse() {
        debug!("Parsed config");

        let mut dec = toml::Decoder::new(toml::Value::Table(table));
        ChatConfig::decode(&mut dec).expect("Invalid config file")
    } else {
        error!("Failed to parse config file. Error list:");
        for e in parser.errors {
            error!("{}", e);
        }
        panic!("Failed to parse configuration file.");
    }
}


#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct ChatConfig {
    pub users: HashMap<UserId, UserConfig>,
}

/// Represents the configuration for a user.
#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct UserConfig {
    pub nets: HashMap<NetId, IrcNetConfig>,
    /// Command to run when there are no clients to send alerts to.
    pub alert_cmd: Option<String>,
}

/// Represents the configuration for a network.
#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct IrcNetConfig {
    pub irc: Config,
}

impl Default for UserConfig {
    fn default() -> UserConfig {
        UserConfig {
            nets: HashMap::new(),
            alert_cmd: None,
        }
    }
}
