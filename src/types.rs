// lets use surreal db
// this time lets build from the ground up for persistence

use crate::{
    database::{SurrealAsLink, SurrealLink, SurrealTable},
    service::{spotify::SpotifyTrackMetadata, youtube::YoutubeTrackMetadata},
};
use eyre::Result;
use std::path::PathBuf;

use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongId(Uuid);
impl SurrealLink for SongId {
    const NAME: &'static str = "song";
}

pub struct Song {
    // can be simpler since we aren't mapping foreign info
    download: Option<Result<PathBuf>>,
    name: String,
    album: String,
    artist: String,
}

/*
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlbumId(Uuid);
impl SurrealLink for AlbumId {
    const NAME: &'static str = "album";
}
pub struct Album {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistId(Uuid);
impl SurrealLink for ArtistId {
    const NAME: &'static str = "album";
}
pub struct Artist {
    name: String,
}
*/

// MUSIC SERVICE TYPES
#[derive(
    Debug, Clone, PartialEq, Eq, Display, EnumString, DeserializeFromStr, SerializeDisplay,
)]
#[strum(ascii_case_insensitive)]
#[strum(serialize_all = "lowercase")]
pub enum MusicService {
    Spotify,
    Youtube,
    Soundcloud,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackId {
    pub service: MusicService,
    pub id: String,
}

impl SurrealLink for TrackId {
    const NAME: &'static str = "track";
}

/// A track for a single service
/// see `Song` for generic data
pub struct Track {
    id: TrackId,
    metadata: TrackMetaData,
}

pub enum TrackMetaData {
    Spotify(SpotifyTrackMetadata),
    Youtube(YoutubeTrackMetadata),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionId {
    pub service: MusicService,
    pub id: String,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    id: CollectionId,
    kind: Kind,
    owner: String,
    size: usize,
    ignored: bool,
    rev: String,
    date: DateTime<Utc>,
    name: String,

    /// Do we expect it to change?
    expect_static: bool,

    #[serde_as(as = "Vec<SurrealAsLink>")]
    tracks: Vec<TrackId>,
}

// CHAT SERVICE TYPES
#[derive(
    Debug, Clone, PartialEq, Eq, Display, EnumString, DeserializeFromStr, SerializeDisplay,
)]
#[strum(ascii_case_insensitive)]
#[strum(serialize_all = "lowercase")]
pub enum ChatService {
    Discord,
    Matrix,
}

#[derive(Debug, Clone, EnumString, Display, DeserializeFromStr, SerializeDisplay)]
#[strum(ascii_case_insensitive)]
#[strum(serialize_all = "lowercase")]
pub enum Kind {
    Artist,
    Album,
    Track,
    Playlist,
    User,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelId {
    pub service: ChatService,

    pub id: String,
}

//TODO to_string/from_string impl which will make json config easier while still expanding data in surreal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderId {
    pub service: ChatService,
    pub id: String,
}
impl SurrealLink for SenderId {
    const NAME: &'static str = "sender";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageId(pub String);
impl SurrealLink for MessageId {
    const NAME: &'static str = "message";
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub channel: ChannelId,

    #[serde_as(as = "SurrealAsLink")]
    pub sender: SenderId,
    pub date: DateTime<Utc>,
    pub links: Vec<Link>,
}
impl SurrealTable for Message {
    const NAME: &'static str = "message";
    // fn id(&self) -> Option<Id> {
    //     Some(self.id.clone().into())
    // }
}
/// This data will actually be stuck into a relation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Link {
    pub service: MusicService,
    pub id: String,
    pub kind: Option<Kind>,
    pub url: Url,
}

// needs to stored with RELATE
#[derive(Debug, Clone)]
pub struct Reaction {
    pub sender: SenderId,
    pub target: MessageId,
    pub date: DateTime<Utc>,
    pub id: MessageId,

    pub txt: Vec<String>, //Normally single, but lets support multible for the hell of it.
}
