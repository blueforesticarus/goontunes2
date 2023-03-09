use crate::service::matrix::MatrixConfig;
use serde::{Deserialize, Serialize};
use std::fs::File;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub services: Vec<ServiceConfig>,
    //playlists: Vec<PlaylistConfig>,
}

impl Config {
    pub fn example() -> Self {
        Config {
            services: vec![ServiceConfig::Matrix(MatrixConfig::example())],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ServiceConfig {
    Matrix(MatrixConfig),
}

struct PlaylistConfig {
    inputs: Vec<PlaylistMember>,
    outputs: Vec<PlaylistMember>,
    sync: Vec<PlaylistMember>, // is input and output (ex. file)

    transforms: (), // filter, transform, bash script
}

enum PlaylistMember {
    //Channel(Channel)
    //Playlist(Playlist)
    //File(File)
}
