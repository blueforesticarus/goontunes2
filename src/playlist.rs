use crate::{
    database::Database,
    types::{ChannelId, Kind, Song},
};
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
            name: "".to_string(),
            inputs: vec![],
            no_repeat: true,
            shuffle: false,
            reverse: false,
            kind: vec![Kind::Track, Kind::Album],
            description: "".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum PlaylistInput {
    Channel(ChannelId),
}

pub struct Playlist {
    pub config: PlaylistConfig,
    pub tracks: Vec<Song>,   //TODO metatrack
    pub date: DateTime<Utc>, //XXX should be monotonic?
}

impl PlaylistConfig {
    fn build(db: &Database) -> Playlist {
        db.db.query("SELECT links FROM message ORDER BY date ASC");
    }
}
