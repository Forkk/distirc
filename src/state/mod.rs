//! An API for interacting with the core's state

use std::collections::hash_map;
use std::collections::HashMap;

use user::{UserId, User};
use config::UserConfig;

mod user;

pub use self::user::{UserHandle, UserClientHandle};


/// Container for the core's state.
///
/// This stores all of the users and their networks and their buffers and
/// provides a nice API for accessing them.
pub struct Core {
    users: HashMap<UserId, UserHandle>,
}


impl Core {
    /// Creates a new core state with no users.
    pub fn new() -> Core {
        Core {
            users: HashMap::new(),
        }
    }

    /// Adds a new user with the given ID and configuration to the core.
    pub fn add_user(&mut self, id: UserId, cfg: UserConfig) {
        let user = User::from_cfg(cfg);
        let handle = UserHandle::new(user);
        self.users.insert(id, handle);
    }


    /// Returns an iterator over all of the users.
    pub fn iter_users(&self) -> IterUsers {
        self.users.iter()
    }

    /// Gets a reference to a user handle for the user with the given ID if one
    /// exists.
    pub fn get_user(&self, id: &UserId) -> Option<&UserHandle> {
        self.users.get(id)
    }

    /// Gets a mutable reference to a user handle for the user with the given ID
    /// if one exists.
    pub fn get_user_mut(&mut self, id: &UserId) -> Option<&mut UserHandle> {
        self.users.get_mut(id)
    }
}

pub type IterUsers<'a> = hash_map::Iter<'a, UserId, UserHandle>;
