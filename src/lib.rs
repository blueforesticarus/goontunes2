#![feature(async_closure)]
#![feature(associated_type_defaults)]

use serde::{Deserialize, Serialize};
use service::matrix::MatrixConfig;
use std::fs::File;

pub mod config;
pub mod database;
pub mod traits;
pub mod types;

pub mod utils {
    pub mod channel;
    pub mod links;
}
pub mod service {
    pub mod matrix;
}
