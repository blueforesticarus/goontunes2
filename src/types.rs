// lets use surreal db
// this time lets build from the ground up for persistence

use crate::database::{SurrealAsLink, SurrealLink};
use derivative::Derivative;
use eyre::Result;
use std::path::PathBuf;
use surrealdb::sql;

use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongId(Uuid);
impl SurrealLink for SongId {
    const TABLE: &'static str = "song";
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Uri(pub String);

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
pub enum PlaylistTrackError {
    Missing,
    NotTrack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackId {
    pub service: MusicService,
    pub id: String,
}

impl SurrealLink for TrackId {
    const TABLE: &'static str = "track";
}

/// A track for a single service
/// see `Song` for generic data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
}

// pub enum TrackMetaData {
//     Spotify(SpotifyTrackMetadata),
//     Youtube(YoutubeTrackMetadata),
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionId {
    pub service: MusicService,
    pub id: String,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection<T = TrackId> {
    pub id: Uri,
    pub kind: Kind,
    //pub owner: String,
    //pub size: usize,
    //pub ignored: bool,
    //pub rev: String,
    //pub date: DateTime<Utc>,
    pub name: String,

    /// Do we expect it to change?
    //pub expect_static: bool,

    //#[serde_as(as = "Vec<SurrealAsLink>")]
    pub tracks: Vec<Result<T, PlaylistTrackError>>,
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
impl SurrealLink for ChannelId {
    const TABLE: &'static str = "channel";
}

//TODO to_string/from_string impl which will make json config easier while still expanding data in surreal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderId {
    pub service: ChatService,
    pub id: String,
}
impl SurrealLink for SenderId {
    const TABLE: &'static str = "sender";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageId(pub String);
impl SurrealLink for MessageId {
    const TABLE: &'static str = "message";
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde_as(as = "SurrealAsLink")]
    pub id: MessageId,

    #[serde_as(as = "SurrealAsLink")]
    pub channel: ChannelId,

    #[serde_as(as = "SurrealAsLink")]
    pub sender: SenderId,
    pub date: DateTime<Utc>,

    #[serde(skip_serializing)]
    pub links: Vec<Link>,
}

/// This data will actually be stuck into a relation
#[derive(Derivative, Clone, Deserialize, Serialize)]
#[derivative(Debug)]
pub struct Link {
    pub service: MusicService,
    pub id: String,
    pub kind: Option<Kind>,

    #[derivative(Debug(format_with = "urlfmt"))]
    pub url: Url,
}

fn urlfmt(url: &Url, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
    write!(f, "Url(\"{}\")", url.as_str())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionId(pub String);
impl SurrealLink for ReactionId {
    const TABLE: &'static str = "reaction";
}

// needs to stored with RELATE
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    #[serde_as(as = "SurrealAsLink")]
    pub id: ReactionId,

    #[serde_as(as = "SurrealAsLink")]
    pub sender: SenderId,
    #[serde_as(as = "SurrealAsLink")]
    pub target: MessageId,

    pub date: DateTime<Utc>,

    pub txt: Vec<String>, //Normally single, but lets support multible for the hell of it.
}

type Png = Vec<u8>;
#[derive(Debug, Clone)]
pub struct Sender {
    pub id: SenderId,
    pub alias: Vec<String>,
    pub avatar: Option<Png>,
}

mod examples {
    use crate::traits::Example;

    use super::*;

    impl Example for Message {
        fn example() -> Self {
            Message {
                id: MessageId::example(),
                channel: ChannelId::example(),
                sender: SenderId::example(),
                date: DateTime::example(),
                links: vec![Link::example()],
            }
        }
    }

    impl Example for Reaction {
        fn example() -> Self {
            Reaction {
                date: DateTime::example(),
                id: ReactionId::example(),
                sender: SenderId::example(),
                target: MessageId::example(),
                txt: vec!["👍".to_string()],
            }
        }
    }

    impl Example for Link {
        fn example() -> Self {
            Link {
                service: MusicService::Spotify,
                id: "7qo1SVGPYmkt5eYJSNaqEP".into(),
                kind: Some(Kind::Track),
                url: Url::try_from(
                    "https://open.spotify.com/track/7qo1SVGPYmkt5eYJSNaqEP?si=f0e5baf03b6f4d34",
                )
                .unwrap(),
            }
        }
    }
    impl Example for MessageId {
        fn example() -> Self {
            Self("$Rg2llRNuHiVIRvxxVJm11TBzO54O4lVbiPCJbXti7Xg".into())
        }
    }

    impl Example for ReactionId {
        fn example() -> Self {
            Self("$BzO54O4lVbiPCJbXtiefeTs6llRNuHiVIRvxxsdf11T".into())
        }
    }

    impl Example for ChannelId {
        fn example() -> Self {
            ChannelId {
                service: ChatService::Matrix,
                id: "!xzXpBNaPblPrPkTPYb:matrix.org".into(),
            }
        }
    }
    impl Example for SenderId {
        fn example() -> Self {
            SenderId {
                service: ChatService::Matrix,
                id: "@segfau1t:matrix.org".into(),
            }
        }
    }
}
