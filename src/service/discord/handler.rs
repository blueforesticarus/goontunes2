
use parking_lot::Mutex;
use postage::{sink::Sink, stream::Stream};
use serenity::{
    all::CacheHttp,
    client::{ClientBuilder, Context, EventHandler},
    model::{
        channel::{Message, Reaction},
        gateway::{GatewayIntents, Ready},
    },
};
use tracing::info;

pub use crate::prelude::*;
use crate::utils::when_even::{Loggable, OnError};

use super::db::add_message;

impl super::Module {
    #[throws(eyre::Report)]
    pub async fn listen(self: &Arc<Self>) {
        // These are only relevant to events received, not http api
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let (tx, mut rx) = postage::oneshot::channel();
        let mut client = ClientBuilder::new(&self.config.token, intents)
            .event_handler(Handler(self.clone(), tx.into()))
            .await?;

        self.http.set(client.http.clone()).unwrap();

        tokio::spawn(async move {
            client.start().await.log::<OnError>().unwrap();
        });

        // wait for startup to finish
        rx.recv().await;

        info!("discord listening");
    }
}

type Meme = Mutex<postage::oneshot::Sender<()>>;
struct Handler(Arc<super::Module>, Meme);

#[async_trait]
impl EventHandler for Handler {
    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, ctx: Context, ready: Ready) {
        self.1.lock().try_send(()).unwrap();
        info!("discord connected");
    }

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

        // TODO: how can channel not be in database already?
        //       really what is needed is logic to update stale data like if name changes
        // if let Ok(channel) = msg.channel(ctx).await.log::<OnError>() {
        //     add_channel(&self.0.db, channel)
        //         .await
        //         .log_and_drop::<OnError>();
        // }

        add_message(&self.0.db, msg).await.log_and_drop::<OnError>();
    }

    async fn reaction_add(&self, ctx: Context, react: Reaction) {}
}
