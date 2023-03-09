use chrono::{DateTime, Utc};
// While exploring, remove for prod.
use async_trait::async_trait;
use eyre::{anyhow, Result};
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

/// A serde_as mixin for Links to other tables
pub struct SurrealLink;
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

/// convenience methods for a struct which is to be stored in an surreal table
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

mod sender_impls {
    use serde_json::json;
    use surrealdb::sql::{Array, Id, Thing};

    use crate::types::Sender;

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

async fn init_database() {}
