use itertools::Itertools;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "service")]
pub enum ServiceConfig {
    Discord(crate::service::discord::Config),
    Matrix(crate::service::matrix::Config),
    Spotify(crate::service::spotify::Config),
}

// Config is another place a provider pattern AKA associative product types, would be good
// You could just pass Config as database::Config, for example
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub services: Vec<ServiceConfig>,
    //playlists: Vec<PlaylistConfig>,
    pub database: crate::database::Config,
}

impl Config {
    pub fn get_service<'a, T: TryFrom<&'a ServiceConfig>>(&'a self) -> Vec<T> {
        self.services
            .iter()
            .filter_map(|conf| T::try_from(conf).ok())
            .collect_vec()
    }
}
