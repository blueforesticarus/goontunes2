//#![allow(dead_code)]
#![feature(associated_type_defaults)]
#![feature(try_blocks)]
#![feature(extract_if)]

//pub mod config;
pub mod database;
//pub use database::types;

pub mod config;
//pub mod playlist;
//pub mod traits;
pub mod types;

pub mod utils {
    //pub mod channel;
    pub mod diff;
    pub mod links;
    //pub mod takecell;
    pub mod synctron;
    pub mod when_even;
}

pub mod service {
    pub mod discord;
    pub mod matrix;
    pub mod spotify;
    //pub mod youtube;
}

pub mod prelude {
    pub use async_trait::async_trait;
    pub use chrono::Utc;
    pub type DateTime<U = Utc> = chrono::DateTime<U>;
    pub use serde::{Deserialize, Serialize};
    pub use std::fmt::Display;
    pub use std::sync::Arc;
    pub use url::Url;
    pub type Result<T = (), E = eyre::Report> = std::result::Result<T, E>;
    pub use culpa::{throw, throws};
    pub use std::error::Error;

    pub use crate::database::MyDb;
    pub use crate::types;
    pub use crate::utils::when_even::{Bug, Loggable};
}
