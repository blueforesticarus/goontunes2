// design question: convert to common format in discord module or in database
// whether to store full serenity objects or normalized ones
// common datatype could be a trait defined for each chat module's types or a struct with into defined.
// but really, we need a common struct, unfortionately

use crate::types::chat::*;
pub use serenity::model::prelude as discord;
use surrealdb::{RecordId, RecordIdKey};

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
