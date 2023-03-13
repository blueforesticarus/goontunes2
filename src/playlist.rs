use crate::types::{ChannelId, Kind, Song};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaylistConfig {
    name: String,
    inputs: Vec<PlaylistInput>,

    no_repeat: bool,
    shuffle: bool,
    reverse: bool,

    kind: Vec<Kind>,

    //filter: Vec<Filter>, TODO (reacts!)
    description: String,
}

impl Default for PlaylistConfig {
    fn default() -> Self {
        Self {
            name: "<name>".to_string(),
            inputs: vec![PlaylistInput::All],
            no_repeat: true,
            shuffle: false,
            reverse: false,
            kind: vec![Kind::Track, Kind::Album],
            description: "<description>".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PlaylistInput {
    Channel(ChannelId),
    All,
}

struct Playlist {
    config: PlaylistConfig,
    tracks: Vec<Song>,   //TODO metatrack
    date: DateTime<Utc>, //XXX should be monotonic?
}

impl PlaylistConfig {
    fn build() {
        //do query
    }

    fn query() {
        //build query
    }
}
