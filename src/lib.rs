#![feature(async_closure)]
#![feature(associated_type_defaults)]
#![feature(min_specialization)]
#![feature(try_blocks)]
//#![allow(dead_code)]

pub mod config;
pub mod database;
pub mod playlist;
pub mod traits;
pub mod types;

pub mod utils {
    pub mod channel;
    pub mod diff;
    pub mod links;
    pub mod takecell;
    pub mod when_even;
}
pub mod service {
    //pub mod discord;
    pub mod matrix;
    pub mod spotify;
    pub mod youtube;
}
