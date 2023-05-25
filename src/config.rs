use crate::{
    service::{self, matrix::MatrixConfig},
    traits::Example,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub services: Vec<ServiceConfig>,
    //playlists: Vec<PlaylistConfig>,
}

impl Config {
    pub fn example() -> Self {
        Config {
            services: vec![
                MatrixConfig::example().into(),
                service::spotify::client::Config::example().into(),
            ],
        }
    }

    pub fn get_service<'a, T: TryFrom<&'a ServiceConfig>>(&'a self) -> Vec<T> {
        self.services
            .iter()
            .filter_map(|conf| T::try_from(conf).ok())
            .collect_vec()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, derive_more::TryInto, derive_more::From)]
#[try_into(owned, ref, ref_mut)]
pub enum ServiceConfig {
    Matrix(MatrixConfig),
    Spotify(service::spotify::client::Config),
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
