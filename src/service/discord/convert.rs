// design question: convert to common format in discord module or in database
// whether to store full serenity objects or normalized ones
// common datatype could be a trait defined for each chat module's types or a struct with into defined.
// but really, we need a common struct, unfortionately

use crate::{
    types::chat::*,
    utils::{links::extract_links, pubsub::Topic},
};
use jaq_core::val;
pub use serenity::model::prelude as discord;
use surrealdb::{RecordId, RecordIdKey};

use chrono::{DateTime, Utc};

use super::types::Link;

impl Topic for Vec<MessageBundle> {}

impl From<&discord::Message> for MessageBundle {
    fn from(value: &discord::Message) -> Self {
        Self {
            service: Service::Discord,
            id: value.id.to_string(),
            timestamp: value.timestamp.to_utc(),
            content: value.content.clone(),
            links: extract_links(&value.content),
            username: value.author.name.clone(),
            user_id: value.author.id.to_string(),
            channel_id: value.channel_id.to_string(),
        }
    }
}

impl From<&discord::Message> for Message {
    fn from(value: &discord::Message) -> Self {
        Self {
            text: value.content.clone(),
            timestamp: value.timestamp.to_utc(),
        }
    }
}

impl From<&discord::User> for User {
    fn from(value: &discord::User) -> Self {
        Self {
            name: value.name.clone(),
        }
    }
}

impl From<&discord::Channel> for Channel {
    fn from(value: &discord::Channel) -> Self {
        Self {
            name: match value {
                discord::Channel::Guild(c) => c.name.clone(),
                discord::Channel::Private(c) => c.name(),
                _ => todo!(),
            },
        }
    }
}

pub trait ToSurreal: Sized {
    const TABLE: &'static str;
    fn to_thing(&self) -> RecordId {
        RecordId::from((Self::TABLE, self.to_id()))
    }

    fn to_id(&self) -> RecordIdKey;
}

macro_rules! impl_from_surreal {
    ($table:expr, $t2:ty) => {
        impl ToSurreal for $t2 {
            const TABLE: &'static str = $table;

            fn to_id(&self) -> RecordIdKey {
                let n = self.get() as i64;
                n.into()
            }
        }
    };
}

impl_from_surreal!("user", discord::UserId);
impl_from_surreal!("message", discord::MessageId);
impl_from_surreal!("channel", discord::ChannelId);
impl_from_surreal!("guild", discord::GuildId);

// user
// message
// channel
// reaction

// profile picture
