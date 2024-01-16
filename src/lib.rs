//#![allow(dead_code)]

//pub mod config;
pub mod database;
pub use database::types;

pub mod config;
//pub mod playlist;
//pub mod traits;
//pub mod types;

pub mod utils {
    //pub mod channel;
    //pub mod diff;
    pub mod links;
    //pub mod takecell;
    pub mod synctron;
    pub mod when_even;
}

pub mod service {
    pub mod discord;
    //pub mod matrix;
    pub mod spotify;
    //pub mod youtube;
}
