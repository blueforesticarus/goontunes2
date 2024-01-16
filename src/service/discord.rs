use std::pin::pin;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use eyre::Result;
use futures::{StreamExt, TryStreamExt};
use itertools::Itertools;
use serenity::{
    model::prelude::{Message, Reaction, Ready},
    prelude::{Context, EventHandler, GatewayIntents},
    Client,
};

use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

use crate::{database::Database, types};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// discord bot token
    token: String,

    /// channels
    channels: Vec<String>,
}

pub async fn init(config: DiscordConfig, db: Database) {
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Avoid circular Arc
    let handler = Handler {
        db: db.clone(),
        config: config.clone(),
    };
    let mut client = Client::builder(&config.token, intents)
        .event_handler(handler)
        .await
        .expect("Err creating client");

    //client.cache_and_http.http.get_messages(channel_id, query);
    let http = client.cache_and_http.clone();
    tokio::spawn(async move {
        client.start().await.unwrap();
    });

    //TODO return http somehow
}

pub struct Handler {
    db: Database,
    config: DiscordConfig,
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content == "!ping" {
            // Sending a message can fail, due to a network error, an
            // authentication error, or lack of permissions to post in the
            // channel, so log to stdout when some error happens, with a
            // description of it.
            if let Err(why) = msg.channel_id.say(&ctx.http, "Pong!").await {
                println!("Error sending message: {:?}", why);
            }
        } else if self.process_message(msg.clone()).await {
            println!("{} {}", msg.author.name, msg.content);
        }
    }

    async fn reaction_add(&self, ctx: Context, add_reaction: Reaction) {
        dbg!(add_reaction);
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        // scan history
        for channel in &self.config.channels {
            let timestamp = self.db.most_recent(channel.clone()).await.unwrap();

            let channel = ctx
                .http
                .get_channel(channel.parse().unwrap())
                .await
                .unwrap();

            let mut stream = pin!(channel.id().messages_iter(&ctx.http));
            while let Some(msg) = stream.next().await {
                let msg = msg.unwrap();

                let ts: DateTime<Utc> = *msg.timestamp;
                if let Some(ts_last) = &timestamp {
                    if ts <= *ts_last {
                        break;
                    }
                }

                self.process_message(msg).await;
            }
        }
    }
}

impl Handler {
    async fn process_message(&self, msg: Message) -> bool {
        let links = crate::utils::links::extract_links(msg.content);
        if !links.is_empty() {
            let res = self
                .db
                .add_message(types::Message {
                    id: Thing {
                        tb: "message".to_string(),
                        id: msg.id.to_string().into(),
                    },
                    sender: Thing {
                        tb: "account".to_string(),
                        id: msg.author.id.to_string().into(),
                    },
                    channel: Thing {
                        tb: "channel".to_string(),
                        id: msg.channel_id.to_string().into(),
                    },
                    date: *msg.timestamp,
                    links,
                })
                .await;

            res.unwrap();

            true
        } else {
            false
        }
    }
}
