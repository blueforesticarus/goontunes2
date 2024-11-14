use crate::types::chat::*;
use crate::utils::links::extract_links;
use crate::{prelude::*, service::discord::convert::ToSurreal};

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serenity::model::prelude as discord;
use surrealdb::{RecordId, Value};

use crate::database::{Database, MyDb};

use eyre::Error;

// TODO handle images
// TODO move handling of link out of discord module, make generic filter interface before stuff goes into db
#[throws]
pub async fn add_message(db: &MyDb, msg: discord::Message) {
    let query = r#"
        BEGIN TRANSACTION;
        UPSERT type::thing($id) MERGE $bundle ;
        UPSERT type::thing($id2) MERGE $bundle2 ;
        UPDATE type::thing($id) SET link = $link ;
        RELATE $id->link->$id3 ;
        COMMIT TRANSACTION;
    "#;

    #[derive(Debug, Serialize, Deserialize)]
    struct MsgBundle {
        service: Service,
        message: Message,
        user: RecordId,
        channel: RecordId,
        //discord_message: discord::Message,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct UserBundle {
        user: User,
        service: Service,
        avatar: Option<Avatar>,
        //discord_user: discord::User,
    }

    let links = extract_links(msg.content.as_str());
    if links.is_empty() {
        return ();
    }

    let mut a = db
        .query(query)
        .bind(("id", msg.id.to_thing()))
        .bind((
            "bundle",
            MsgBundle {
                service: Service::Discord,
                message: Message::from(&msg),
                user: msg.author.id.to_thing(),
                channel: msg.channel_id.to_thing(),
                //discord_message: msg.clone(),
            },
        ))
        .bind(("id2", msg.author.id.to_thing()))
        .bind((
            "bundle2",
            UserBundle {
                service: Service::Discord,
                user: User::from(&msg.author),
                //discord_user: msg.author.clone(),
                avatar: None,
            },
        ))
        .bind(("id3", links.iter().map(|l| l.to_thing()).collect_vec()))
        .bind(("link", links))
        .await?;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestBundle {
        user: User,
        discord_user: discord::User,
    }

    a.take::<Option<MsgBundle>>(0)?;
    //a.take::<Option<TestBundle>>(1)?;

    dbg!(msg.content);
}

pub async fn add_channel(db: &MyDb, channel: discord::Channel) -> eyre::Result<()> {
    #[derive(Debug, Serialize, Deserialize)]
    struct ChannelBundle {
        service: Service,
        channel: Channel,
        discord_channel: discord::Channel,
    }

    let data = ChannelBundle {
        service: Service::Discord,
        channel: Channel::from(&channel),
        discord_channel: channel.clone(),
    };
    let _: Option<ChannelBundle> = db.update(channel.id().to_thing()).merge(data).await?;

    Ok(())
}

pub async fn add_guild(db: &MyDb, guild: discord::GuildInfo) -> eyre::Result<()> {
    #[derive(Debug, Serialize, Deserialize)]
    struct GuildBundle {
        service: Service,
        discord_guild: discord::GuildInfo,
    }

    let data = GuildBundle {
        service: Service::Discord,
        discord_guild: guild.clone(),
    };
    let _: Option<GuildBundle> = db.update(guild.id.to_thing()).merge(data).await?;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageIdDatetimeBundle {
    pub ts: DateTime<Utc>,
    pub id: discord::MessageId,
}

pub async fn get_last_message_for_channel(
    db: &MyDb,
    channel: discord::ChannelId,
) -> eyre::Result<Option<MessageIdDatetimeBundle>> {
    let query = r#"
        SELECT message.timestamp as ts, record::id(id) as id
        FROM message
        where channel = $channel
        ORDER BY ts DESC LIMIT 1
    "#;

    let r: Option<MessageIdDatetimeBundle> = db
        .query(query)
        .bind(("channel", channel.to_thing()))
        .await?
        .take(0)?;

    dbg!(&r);

    Ok(r)
}
