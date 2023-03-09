// lets use surreal db
// this time lets build from the ground up for persistence

use crate::database::SurrealLink;

use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};
use url::Url;

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
pub struct Channel {
    pub service: ChatService,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct Sender {
    pub service: ChatService,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageId(String);

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub channel: Channel,

    #[serde_as(as = "SurrealLink")]
    pub sender: Sender,
    pub date: DateTime<Utc>,
    pub text: Option<String>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Link {
    pub service: MusicService,
    pub url: Url,
    pub id: String,
    pub kind: Option<Kind>,
}

// needs to stored with RELATE
#[derive(Debug, Clone)]
pub struct Reaction {
    pub sender: Sender,
    pub target: MessageId,
    pub date: DateTime<Utc>,
    pub id: MessageId,

    pub txt: Vec<String>, //Normally single, but lets support multible for the hell of it.
}
