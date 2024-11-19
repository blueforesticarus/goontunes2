// I have decided this is needed
// I will define the members first. maybe even keep members private and only define methods or some other crap
// note that rust needs equivication of struct and trait.

// biggest issue here is id's
// use std::marker::PhantomData;

// #[derive(Debug, Component)]
// pub struct Id<T = ()> {
//     pub id: u64, //TODO abstract into IdentityDomains
//     pub _phantom_data: PhantomData<T>,
// }

// impl<T> Id<T> {
//     pub fn new(v: u64) -> Self {
//         Self {
//             id: v,
//             _phantom_data: PhantomData,
//         }
//     }
// }

// #[derive(Debug, Component)]
// pub struct Object<T = ()> {
//     pub id: Id<T>,
//     pub data: T,
// }

// #[derive(Debug, Component)]
// pub enum IdObject<T = ()> {
//     Id(Id<T>),
//     Object(Object<T>),
// }
// impl<T> From<IdObject<T>> for Id<T> {
//     fn from(value: IdObject<T>) -> Self {
//         match value {
//             IdObject::Id(id) => id,
//             IdObject::Object(obj) => obj.id,
//         }
//     }
// }

// trait ToIdObject<T>: Sized {
//     fn as_id(&self) -> Id<T>;
//     fn as_obj(self) -> Option<T> {
//         None
//     }

//     fn as_idobj(self) -> IdObject<T> {
//         let id = self.as_id();
//         match self.as_obj() {
//             Some(data) => IdObject::Object(Object { id, data }),
//             None => IdObject::Id(id),
//         }
//     }
// }

// IDEA: identity domains
// a trait to help moving between multiple identity domains
// examples discord ID, bevy entity ID, and surreal ID

// IDEA: cache and client methods to lift ID into object

pub mod chat {
    use super::*;

    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    pub enum Service {
        Discord,
        Matrix,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct User {
        pub name: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Avatar {
        pub url: Url,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Channel {
        pub name: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Message {
        pub text: String,
        pub timestamp: DateTime<Utc>,
    }
    #[derive(Debug, Clone)]
    pub struct MessageBundle {
        pub service: Service,

        pub id: String,
        pub timestamp: DateTime<Utc>,
        pub content: String,
        pub links: Vec<Link>,

        pub username: String,
        pub user_id: String,

        pub channel_id: String,
    }
}

pub mod music {
    use super::*;

    #[derive(
        Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, DeserializeFromStr, SerializeDisplay,
    )]
    #[strum(ascii_case_insensitive)]
    #[strum(serialize_all = "lowercase")]
    pub enum Service {
        Spotify,
        Youtube,
        Soundcloud,
    }
}

pub struct _Link;

// group of users across chat platforms
pub struct _Person;

use chrono::{DateTime, Utc};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};
use surrealdb::RecordId;
use url::Url;

/// This data will actually be stuck into a relation
#[derive(Derivative, Clone, Deserialize, Serialize)]
#[derivative(Debug)]
pub struct Link {
    pub service: music::Service,
    pub id: String,
    pub kind: Option<Kind>,

    #[derivative(Debug(format_with = "urlfmt"))]
    pub url: Url,
}

impl Link {
    pub fn to_thing(&self) -> RecordId {
        let table = match self.kind {
            Some(Kind::Track) => "track",
            Some(Kind::Album) => "album",
            Some(Kind::Playlist) => "playlist",
            Some(Kind::Artist) => "artist",
            Some(Kind::User) => "playlist",
            None => "unknown",
        };
        RecordId::from_table_key(table, &self.id)
    }
}

fn urlfmt(url: &Url, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
    write!(f, "Url(\"{}\")", url.as_str())
}

#[derive(Debug, Copy, Clone, EnumString, Display, DeserializeFromStr, SerializeDisplay)]
#[strum(ascii_case_insensitive)]
#[strum(serialize_all = "lowercase")]
pub enum Kind {
    Artist,
    Album,
    Track,
    Playlist,
    User,
}
