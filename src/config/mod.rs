use std::collections::HashMap;
use irc::client::prelude::Config;

/// Represents the configuration for a user.
#[derive(Debug, Clone)]
pub struct UserConfig {
    pub name: String,
    pub networks: HashMap<String, IrcNetConfig>,
}

/// Represents the configuration for a network.
#[derive(Debug, Clone)]
pub struct IrcNetConfig(pub Config);


