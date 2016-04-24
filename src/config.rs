use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::collections::HashMap;
use std::default::Default;
use toml;
use toml::Parser;
use irc::client::prelude::Config as IrcLibConfig;
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
    pub user: HashMap<UserId, UserConfig>,
}

/// Represents the configuration for a user.
#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct UserConfig {
    pub net: HashMap<NetId, NetConfig>,
    /// Password to authenticate as this user.
    pub password: String,
    /// Command to run when there are no clients to send alerts to.
    pub alert_cmd: Option<String>,
}


#[derive(Debug, Clone, RustcEncodable, RustcDecodable)]
pub struct NetConfig {
    nick: String,
    alt_nicks: Vec<String>,
    nickserv_pass: Option<String>,
    channels: Vec<String>,
    username: Option<String>,
    realname: Option<String>,

    // Server options
    server: String,
    port: Option<u16>,
    password: Option<String>,
    use_ssl: Option<bool>,
}

impl NetConfig {
    pub fn nick(&self) -> &str { &self.nick }
    pub fn alt_nicks(&self) -> Vec<String> {
        self.alt_nicks.iter().map(|n| n.clone()).collect()
    }
    pub fn username(&self) -> &str {
        self.username.as_ref().map_or(self.nick(), |n| &n[..])
    }
    pub fn realname(&self) -> &str {
        self.realname.as_ref().map_or(self.nick(), |n| &n[..])
    }
    pub fn nickserv_pass(&self) -> Option<&str> {
        self.nickserv_pass.as_ref().map(|n| &n[..])
    }

    pub fn server(&self) -> &str { &self.server }
    pub fn port(&self) -> u16 { self.port.unwrap_or(6667) }
    pub fn channels(&self) -> Vec<String> {
        self.channels.iter().map(|n| n.clone()).collect()
    }
}


impl NetConfig {
    /// Generates an `IrcLibConfig` struct from this config.
    pub fn irc_config(&self) -> IrcLibConfig {
        IrcLibConfig {
            nickname: Some(self.nick().to_owned()),
            alt_nicks: Some(self.alt_nicks()),
            nick_password: self.nickserv_pass.clone(),
            username: Some(self.username().to_owned()),
            realname: Some(self.realname().to_owned()),
            channels: Some(self.channels()),

            server: Some(self.server().to_owned()),
            port: Some(self.port()),
            .. IrcLibConfig::default()
        }
    }
}


impl Default for UserConfig {
    fn default() -> UserConfig {
        UserConfig {
            net: HashMap::new(),
            password: String::new(),
            alert_cmd: None,
        }
    }
}
