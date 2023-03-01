#![allow(unused)]
use chrono::{DateTime, Utc};
// While exploring, remove for prod.
use eyre::{anyhow, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use serde_with::{
    serde_as, DeserializeAs, DeserializeFromStr, SerializeAs, SerializeDisplay, TryFromInto,
};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;
use strum::{Display, EnumString};
use surrealdb::dbs::{Response, Session};
use surrealdb::engine::local::Mem;
use surrealdb::kvs::Datastore;
use surrealdb::method::UseNsDb;
use surrealdb::opt::from_json;
use surrealdb::sql::{thing, Array, Datetime, Id, Object, Part, Strand, Table, Thing, Value};
use surrealdb::{Connection, Surreal};
use url::Url;

type DB = (Datastore, Session);

#[tokio::main]
async fn main() -> Result<()> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("default").use_db("default").await?;

    /*
    dbg!(db.query("CREATE mytable SET id = 'AAA', data = 5").await?);
    dbg!(
        db.query("CREATE mytable:{ a: '123', b: 3 } SET id = 'AAA', data = 5, a = '123', b= 3")
            .await?
    );
    */

    // --- Create
    let t1 = Message::create(
        &db,
        Message {
            channel: Channel {
                service: ChatService::Discord,
                id: "club cyberia".to_string(),
            },
            id: "testmessageid".into(),
            sender: Sender {
                service: ChatService::Discord,
                id: "sushidude".to_string(),
            },
            date: Utc::now(),
            links: vec![Link {
                service: MusicService::Youtube,
                url: Url::parse("http://youtube.com").unwrap(),
                kind: None,
                id: "blah".to_string(),
            }],
        },
    )
    .await?;
    let t2 = Message::create(
        &db,
        Message {
            channel: Channel {
                service: ChatService::Discord,
                id: "club cyberia".to_string(),
            },
            id: "wer4qwer".into(),
            sender: Sender {
                service: ChatService::Discord,
                id: "segfault".to_string(),
            },
            date: Utc::now(),
            links: vec![Link {
                service: MusicService::Spotify,
                url: Url::parse("http://spotify.com").unwrap(),
                kind: Some(Kind::Track),
                id: "2983472983".to_string(),
            }],
        },
    )
    .await?;

    // --- Select
    let sql = "SELECT * from message";
    let mut res = db.query(sql).await?;
    for object in res.0.remove(&0).unwrap().unwrap() {
        println!("record {}", object);
    }

    Ok(())
}

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

/// Ideally we would not repeat data, however, there is currently no way to use struct info when serializing a field in serde (even with serde_as)
/// Additionally rust has no conventional way for converting subsets of fields in a struct. So instead we repeat the service enum in 3 places: id.service, sender.service, channel.service
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    //#[serde(skip_serializing)]
    pub id: String,
    pub channel: Channel,

    #[serde_as(as = "SurrealLink")]
    pub sender: Sender,
    pub date: DateTime<Utc>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Channel {
    pub service: ChatService,
    pub id: String,
}

#[serde_as]
#[derive(Debug, Clone)]
pub struct Sender {
    pub service: ChatService,
    pub id: String,
}

mod sender_impls {
    use serde_json::json;
    use surrealdb::{
        opt::from_json,
        sql::{Array, Id, Thing},
    };

    use super::Sender;

    impl From<Sender> for Thing {
        fn from(value: Sender) -> Self {
            Self {
                tb: "sender".to_string(),
                id: value.into(),
            }
        }
    }

    impl TryFrom<Thing> for Sender {
        type Error = eyre::Error;

        fn try_from(value: Thing) -> std::result::Result<Self, Self::Error> {
            value.id.try_into()
        }
    }

    impl TryFrom<Id> for Sender {
        type Error = eyre::Error;

        fn try_from(value: Id) -> std::result::Result<Self, Self::Error> {
            if let Id::Array(Array(a)) = value {
                if let [a, b] = &a[..] {
                    return Ok(Self {
                        service: serde_json::from_value(json!(a))?,
                        id: a.to_string(),
                    });
                };
            };
            eyre::bail!("blah")
        }
    }
    impl From<Sender> for Id {
        fn from(value: Sender) -> Self {
            Id::from(vec![value.service.to_string(), value.id])
        }
    }
}

struct SurrealLink;
impl<T> SerializeAs<T> for SurrealLink
where
    T: Into<Thing> + Clone,
{
    fn serialize_as<S>(source: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value: Thing = source.clone().into();
        value.serialize(serializer)
    }
}

impl<'de, E: Display, T: TryFrom<Thing, Error = E>> DeserializeAs<'de, T> for SurrealLink {
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /*
        let thing = Thing::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        Ok(thing.id.to_string())
        */

        let s = String::deserialize(deserializer).map_err(serde::de::Error::custom)?;
        let t = thing(&s).map_err(serde::de::Error::custom)?;
        t.try_into().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Link {
    pub service: MusicService,
    pub url: Url,
    pub id: String,
    pub kind: Option<Kind>,
}

/*
impl From<Link> for Value {
    fn from(value: Link) -> Self {
        let mut map: BTreeMap<String, Value> = BTreeMap::from([
            ("service".into(), value.service.to_string().into()),
            ("url".into(), value.url.to_string().into()),
            ("id".into(), value.id.into()),
        ]);
        if let Some(kind) = value.kind {
            map.insert("kind".into(), kind.to_string().into());
        };

        map.into()
    }
}

impl From<Message> for Value {
    fn from(value: Message) -> Self {
        let mut map: BTreeMap<String, Value> = BTreeMap::from([
            ("service".into(), value.service.to_string().into()),
            ("id".into(), value.id.into()),
            (
                "sender".into(),
                make_link(
                    "sender",
                    vec![value.service.to_string(), value.sender.to_string()],
                )
                .into(),
            ),
            ("date".into(), value.date.to_string().into()),
            //("links".into(), value.links.into()), //TODO fork surreal, add impl so this works
            (
                "links".into(),
                value
                    .links
                    .into_iter()
                    .map(|v| from_json(json!(v)))
                    .collect::<Vec<_>>()
                    .into(),
            ),
        ]);

        map.into()
    }
}
*/

use async_trait::async_trait;
#[async_trait]
pub trait SurrealTable: Serialize + Send + Sized {
    //type Item: Serialize + Send = Self;
    const NAME: &'static str;

    async fn create(db: &Surreal<impl Connection>, msg: Self) -> Result<String> {
        let id: Value = match msg.id() {
            Some(id) => Thing::from((Self::NAME.to_string(), id)).into(),
            None => Table::from(Self::NAME.to_string()).into(),
        };

        let sql = "CREATE $id CONTENT $data";
        let mut response = db.query(sql).bind_raw("id", id).bind(("data", msg)).await?;

        //let v: Vec<Value> = response.take(0)?;
        /*
        Failed to convert `[{ id: task:mbzgfq4g9n41f8hn3do0, priority: 10, title: 'Task 01' }]` to `T`: array had incorrect length, expected 3·
         */

        //changed surreal crate to expose inner map because of above issue
        //let v: Vec<Value> = response.0.remove(&0).unwrap()?;
        let v: Option<String> = response.take("id")?;

        dbg!(&v);
        Ok(v.unwrap())
    }

    fn id(&self) -> Option<Id> {
        None
    }
}

impl SurrealTable for Message {
    const NAME: &'static str = "message";
    // fn id(&self) -> Option<Id> {
    //     Some(self.id.clone().into())
    // }
}
