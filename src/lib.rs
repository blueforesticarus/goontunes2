#![feature(async_closure)]
#![feature(associated_type_defaults)]
#![feature(min_specialization)]

use serde::{Deserialize, Serialize};
use service::matrix::MatrixConfig;
use std::fs::File;

pub mod config;
pub mod database;
pub mod playlist;
pub mod traits;
pub mod types;

pub mod utils {
    pub mod channel;
    pub mod links;
    pub mod takecell;
}
pub mod service {
    pub mod matrix;
    pub mod spotify;
    pub mod youtube;
}
