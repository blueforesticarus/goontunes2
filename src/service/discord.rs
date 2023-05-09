// twilight has no function to extract emoji reacts
// serenity only has query:str for messages pagination
// NOTE, why isn't the data model completely abstracted away, so you just access things and it gets it from http, cache, ws, custom code etc automatically

use async_trait::async_trait;
use eyre::Result;
use serenity::{
    model::prelude::{Message, Reaction, Ready},
    prelude::{Context, EventHandler, GatewayIntents},
    Client,
};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// discord bot token
    token: String,
}

impl DiscordConfig {
    pub fn example() -> Self {
        Self {
            token: "<Token>".to_string(),
        }
    }
}

pub struct DiscordClient {}

impl DiscordClient {
    pub async fn connect(config: DiscordConfig) -> Result<Arc<Self>> {
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        // Avoid circular Arc
        let mut client = Client::builder(&config.token, intents)
            .event_handler()
            .await
            .expect("Err creating client");

        let client: Arc<Self> = Self {}.into();

        Ok(client)
    }
}

struct Handler;

#[async_trait]
impl EventHandler for DiscordClient {
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
    async fn ready(&self, _c: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}
