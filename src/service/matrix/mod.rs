mod config;
mod db;
mod handler;
mod init;
mod process;
mod util;

use std::sync::OnceLock;

pub use config::{AuthConfig, Config};

use crate::prelude::*;

pub struct Module {
    client: OnceLock<matrix_sdk::Client>,
    config: Config,
    db: MyDb,
}

impl Module {
    pub fn new(config: Config, db: MyDb) -> Arc<Self> {
        Self {
            config,
            db,
            client: OnceLock::new(),
        }
        .into()
    }

    pub fn client(&self) -> &matrix_sdk::Client {
        self.client.get().expect("matrix not initialized")
    }
}
