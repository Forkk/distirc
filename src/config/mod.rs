use std::collections::HashMap;
use irc::client::prelude::Config;

use common::types::NetId;

pub type UserId = String;

/// Represents the configuration for a user.
#[derive(Debug, Clone)]
pub struct UserConfig {
    pub name: UserId,
    pub networks: HashMap<NetId, IrcNetConfig>,
}

/// Represents the configuration for a network.
#[derive(Debug, Clone)]
pub struct IrcNetConfig {
    pub name: NetId,
    pub cfg: Config
}
