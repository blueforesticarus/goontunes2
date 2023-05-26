// While exploring, remove for prod.
use async_trait::async_trait;
use eyre::{bail, Context, Result};
use lazy_static::__Deref;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DeserializeAs, SerializeAs};
use std::any::type_name;
use std::sync::Arc;
use surrealdb::engine::local::{Db, Mem};
use tracing_subscriber::fmt::format;

use surrealdb::sql;
use surrealdb::{Connection, Surreal};

pub trait SurrealLink: Sized + Serialize + DeserializeOwned {
    const TABLE: &'static str;

    fn try_to_thing(self) -> Result<sql::Thing> {
        Ok(sql::Thing {
            tb: Self::TABLE.to_string(),
            id: self.try_to_id()?,
        })
    }

    fn try_from_thing(thing: sql::Thing) -> Result<Self> {
        if thing.tb == Self::TABLE {
            Self::try_from_id(thing.id)
        } else {
            bail!("expected table `{}` got `{}`", Self::TABLE, thing.tb)
        }
    }

    fn try_to_id(&self) -> Result<sql::Id> {
        let value = sql::to_value(self)
            .wrap_err_with(|| format!("couldn't convert {} to Value", Self::TABLE))?;
        Ok(match value {
            sql::Value::Number(v) => v.as_int().into(),
            sql::Value::Strand(v) => v.into(),
            sql::Value::Datetime(v) => v.to_raw().into(),
            sql::Value::Uuid(v) => v.into(),
            sql::Value::Array(v) => v.into(),
            sql::Value::Object(v) => v.into(),
            _ => bail!("?? {:?}", value),
        })
    }

    fn try_from_id(id: sql::Id) -> Result<Self> {
        let v = sql::Value::from(id.clone());
        surrealdb::opt::from_value(v).context(format!("{:?} {}", id, type_name::<Self>()))
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
        let value: sql::Thing = source
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

        let v = sql::Thing::deserialize(deserializer).unwrap(); //.map_err(serde::de::Error::custom)?;
        T::try_from_thing(v).map_err(serde::de::Error::custom)
    }
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, derive_more::Deref, derive_more::From)]
pub struct Id<T: SurrealLink + Clone>(#[serde_as(as = "SurrealAsLink")] pub T);

// #[async_trait]
// pub trait SurrealTable: Serialize + Send + Sized {
//     //type Item: Serialize + Send = Self;
//     const NAME: &'static str;

//     async fn create(db: &Surreal<impl Connection>, msg: Self) -> Result<String> {
//         let id: Value = match msg.id() {
//             Some(id) => Thing::from((Self::NAME.to_string(), id)).into(),
//             None => Table::from(Self::NAME.to_string()).into(),
//         };

//         let sql = "CREATE $id CONTENT $data";
//         let mut response = db.query(sql).bind(("id", id)).bind(("data", msg)).await?;

//         //let v: Vec<Value> = response.take(0)?;
//         /*
//         Failed to convert `[{ id: task:mbzgfq4g9n41f8hn3do0, priority: 10, title: 'Task 01' }]` to `T`: array had incorrect length, expected 3·
//          */
//         //changed surreal crate to expose inner map because of above issue
//         //let v: Vec<Value> = response.0.remove(&0).unwrap()?;
//         let v: Option<String> = response.take("id")?;

//         dbg!(&v);
//         Ok(v.unwrap())
//     }

//     fn id(&self) -> Option<Id> {
//         None
//     }
// }

/*
rough idea:

fn add message

fn add reaction

fn get unknown users

fn add user

fn taint channel

fn clean channel

fn init db
 */

use crate::types::{self, Track, TrackId};

#[derive(Debug, Clone)]
pub struct Database {
    db: Arc<Surreal<Db>>,
}

impl Database {
    pub async fn init() -> Database {
        let db = Surreal::new::<Mem>(()).await.unwrap();
        db.use_ns("default").use_db("default").await.unwrap();

        Self { db: db.into() }
    }

    pub async fn add_message(&self, message: types::Message) -> Result<()> {
        let ret: Option<types::Message> = self
            .db
            .update(("message", &message.id.0))
            .content(message.clone())
            .await?;

        for link in message.links {
            self.relate_link(
                message.id.clone(),
                TrackId {
                    service: link.service.clone(),
                    id: link.clone().id,
                },
                link,
            )
            .await
        }

        //dbg!(ret);
        Ok(())
    }

    pub async fn add_reaction(&self, message: types::Reaction) -> Result<()> {
        let ret: Option<types::Reaction> = self
            .db
            .create(("reaction", &message.id.0))
            .content(message)
            .await?;

        //dbg!(ret);
        Ok(())
    }

    pub async fn relate_link(
        &self,
        message: types::MessageId,
        track: types::TrackId,
        link: types::Link,
    ) {
        self.db
            .query("RELATE $message->links->$track CONTENT $link")
            .bind(("message", message))
            .bind(("track", track))
            .bind(("link", link))
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use surrealdb::sql;

    use super::Database;
    use crate::{traits::Example, types};

    async fn test_db() -> Database {
        Database::init().await
    }

    #[tokio::test]
    async fn add_message() -> eyre::Result<()> {
        let v = sql::to_value(types::Message::example());
        dbg!(&v);
        let db = test_db().await;
        db.add_message(types::Message::example()).await
    }

    #[tokio::test]
    async fn add_react() -> eyre::Result<()> {
        let v = sql::to_value(types::Reaction::example());
        dbg!(&v);
        let db = test_db().await;
        db.add_reaction(types::Reaction::example()).await
    }
}
