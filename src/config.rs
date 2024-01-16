use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::service::discord::DiscordConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub services: Vec<ServiceConfig>,
    //playlists: Vec<PlaylistConfig>,
}

impl Config {
    pub fn get_service<'a, T: TryFrom<&'a ServiceConfig>>(&'a self) -> Vec<T> {
        self.services
            .iter()
            .filter_map(|conf| T::try_from(conf).ok())
            .collect_vec()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceConfig {
    Discord(DiscordConfig),
}
