// While exploring, remove for prod.
use async_trait::async_trait;
use eyre::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_with::{DeserializeAs, SerializeAs};
use std::any::type_name;

use surrealdb::opt::from_json;
use surrealdb::sql::{thing, Id, Table, Thing, Value};
use surrealdb::{Connection, Surreal};

pub trait SurrealLink: Sized + Serialize + DeserializeOwned {
    const NAME: &'static str;

    fn try_to_thing(self) -> Result<Thing> {
        Ok(Thing {
            tb: Self::NAME.to_string(),
            id: self.try_to_id()?,
        })
    }

    fn try_from_thing(thing: Thing) -> Result<Self> {
        if thing.tb == Self::NAME {
            Self::try_from_id(thing.id).wrap_err("")
        } else {
            bail!("expected: {}, got:{}", Self::NAME, thing.tb)
        }
    }

    fn try_to_id(&self) -> Result<Id> {
        let value = from_json(serde_json::to_value(self)?);
        Ok(match value {
            Value::Number(v) => v.as_int().into(),
            Value::Strand(v) => v.into(),
            Value::Datetime(v) => v.to_raw().into(),
            Value::Uuid(v) => v.into(),
            Value::Array(v) => v.into(),
            Value::Object(v) => v.into(),
            _ => panic!("?? {:?}", value),
        })
    }

    fn try_from_id(id: Id) -> Result<Self> {
        serde_json::from_value(json!(id)).context(format!("{:?} {}", id, type_name::<Self>()))
    }
}

pub struct SurrealAsLink;
impl<T> SerializeAs<T> for SurrealAsLink
where
    T: SurrealLink + Clone,
{
    fn serialize_as<S>(source: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let value: Thing = source
            .clone()
            .try_to_thing()
            .map_err(serde::ser::Error::custom)?;
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

async fn init_database() {}
