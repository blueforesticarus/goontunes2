use eyre::ContextCompat;



use kameo::actor::ActorRef;
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
use crate::utils::when_even::{Bug, Loggable};

// honestly not sure what I should return here, a Client? an Http?, Context?
// I would think a Client, but it does not implement clone
#[throws(eyre::Report)]
pub async fn connect(config: &super::Config, actor_ref: ActorRef<super::Module>) -> Context {
    // These are only relevant to events received, not http api
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let (tx, mut rx) = postage::oneshot::channel();
    let handler = Handler {
        actor_ref,
        ready_tx: tx.into(),
    };
    let mut client = ClientBuilder::new(&config.token, intents)
        .event_handler(handler)
        .await?;

    //let a = client.http.clone();

    tokio::spawn(async move {
        client.start().await.log::<Bug>().unwrap();
    });

    // wait for startup to finish
    rx.recv().await.context("discord never intialied")?
}

type Meme = Mutex<postage::oneshot::Sender<Context>>;
struct Handler {
    actor_ref: ActorRef<super::Module>,
    ready_tx: Meme,
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, ctx: Context, _ready: Ready) {
        let g = self.ready_tx.lock().try_send(ctx).unwrap();
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
    }

    async fn reaction_add(&self, ctx: Context, react: Reaction) {}
}
