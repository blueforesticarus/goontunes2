#![allow(unused)]
use chrono::{DateTime, Utc};
// While exploring, remove for prod.
use eyre::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use serde_with::{
    serde_as, DeserializeAs, DeserializeFromStr, SerializeAs, SerializeDisplay, TryFromInto,
};
use std::collections::{BTreeMap, HashMap};
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

    // db.create(("test", "testid"))
    //     .content(HashMap::from([("foo", 5)]))
    //     .await?;

    // dbg!(db.query("CREATE mytable SET id = 'AAA', data = 5").await?);
    // dbg!(
    //     db.query("CREATE mytable:{ a: '123', b: 3 } SET id = 'AAA', data = 5")
    //         .await?
    // );
    // dbg!(db.query("SELECT * from mytable WHERE id.b = 3").await?);

    // --- Create
    let t1 = Message::create(
        &db,
        Message {
            channel: Channel {
                service: ChatService::Discord,
                id: "club cyberia".to_string(),
            },
            id: MessageId("testmessageid".into()),
            sender: SenderId {
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
            id: MessageId("wer4qwer".into()),
            sender: SenderId {
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

    let r = Reaction {
        id: MessageId("testreactid".into()),
        sender: SenderId {
            service: ChatService::Discord,
            id: "segfault".to_string(),
        },
        target: MessageId("testmessageid".into()),
        date: Utc::now(),
        txt: vec!["👍".into()],
    };

    let sql = "RELATE $sender->reaction->$message SET date = $date, txt = $txt";
    let mut res = db
        .query(sql)
        .bind(("sender", r.sender.to_thing()))
        .bind(("message", r.target.to_thing()))
        .bind(("data", r.date))
        .bind(("txt", r.txt))
        .await?;

    dbg!(res);

    // --- Select
    let sql = "SELECT * from message WHERE sender = $sender";
    let mut res = db
        .query(sql)
        .bind((
            "sender",
            SenderId {
                service: ChatService::Discord,
                id: "segfault".to_string(),
            }
            .to_thing(),
        ))
        .await?;
    for object in res.0.remove(&0).unwrap().unwrap() {
        println!("{}", object);
        let parsed: Message = serde_json::from_value(json!(object))?;
        dbg!(parsed);
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageId(String);
/// Ideally we would not repeat data, however, there is currently no way to use struct info when serializing a field in serde (even with serde_as)
/// Additionally rust has no conventional way for converting subsets of fields in a struct. So instead we repeat the service enum in 3 places: id.service, sender.service, channel.service
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    //#[serde(skip_serializing)]
    #[serde_as(as = "SurrealAsLink")]
    pub id: MessageId,
    pub channel: Channel,

    #[serde_as(as = "SurrealAsLink")]
    pub sender: SenderId,
    pub date: DateTime<Utc>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Channel {
    pub service: ChatService,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct SenderId {
    pub service: ChatService,
    pub id: String,
}

trait SurrealLink: Sized + Into<Id> + TryFrom<Id, Error = eyre::Error> {
    const NAME: &'static str;

    fn to_thing(self) -> Thing {
        Thing {
            tb: Self::NAME.to_string(),
            id: self.into(),
        }
    }

    fn try_from_thing(thing: Thing) -> Result<Self> {
        if thing.tb == Self::NAME {
            thing.id.try_into().wrap_err("")
        } else {
            bail!("expected: {}, got:{}", Self::NAME, thing.tb)
        }
    }
}

impl SurrealLink for MessageId {
    const NAME: &'static str = "message";
}

impl SurrealLink for SenderId {
    const NAME: &'static str = "sender";
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

mod link_impls {

    use serde_json::json;
    use surrealdb::{
        opt::from_json,
        sql::{Array, Id, Thing, Value},
    };

    use crate::{MessageId, SurrealLink};

    use super::SenderId;

    impl TryFrom<Id> for MessageId {
        type Error = eyre::Error;

        fn try_from(value: Id) -> Result<Self, Self::Error> {
            dbg!(&value);
            dbg!(json!(Value::from(value.clone())));
            let v: Self = serde_json::from_value(json!(Value::from(value)))?;
            Ok(v)
        }

    impl From<MessageId> for Id {
        fn from(value: MessageId) -> Self {
            Id::String(value.0)
        }
    }

    impl TryFrom<Id> for SenderId {
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

    impl From<SenderId> for Id {
        fn from(value: SenderId) -> Self {
            Id::from(vec![value.service.to_string(), value.id])
        }
    }
}

struct SurrealAsLink;
impl<T> SerializeAs<T> for SurrealAsLink
where
    T: SurrealLink + Clone,
{
    fn serialize_as<S>(source: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value: Thing = source.clone().to_thing();
        value.serialize(serializer)
    }
}

impl<'de, T: SurrealLink> DeserializeAs<'de, T> for SurrealAsLink {
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
        T::try_from_thing(t).map_err(serde::de::Error::custom)
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
