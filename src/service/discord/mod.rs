use std::sync::OnceLock;

use futures::{Sink, Stream};
use serenity::http::Http;
use types::Link;

use crate::prelude::*;

mod convert;
mod db;
mod handler;
mod init;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// discord bot token
    token: String,

    /// channels
    channels: Vec<String>,
}

pub struct Module {
    config: Config,
    db: MyDb,
    http: OnceLock<Arc<Http>>,
}

impl Module {
    pub fn new(config: Config, db: MyDb) -> Arc<Self> {
        Self {
            config,
            db,
            http: Default::default(),
        }
        .into()
    }
}
