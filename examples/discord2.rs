use futures::stream::StreamExt;
use std::{env, error::Error, sync::Arc};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Cluster, Event};
use twilight_http::Client as HttpClient;
use twilight_model::gateway::Intents;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    let token = "ODc4MzM3ODUyOTU2NjE4ODAy.GXPbqP.-X9UudO7ezgrwl6nd4kb6w2n5CC7cOTj67UMGQ";

    // Use intents to only receive guild message events.

    // A cluster is a manager for multiple shards that by default
    // creates as many shards as Discord recommends.
    let (cluster, mut events) = Cluster::new(
        token.to_owned(),
        Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT,
    )
    .await?;
    let cluster = Arc::new(cluster);

    // Start up the cluster.
    let cluster_spawn = Arc::clone(&cluster);

    // Start all shards in the cluster in the background.
    tokio::spawn(async move {
        cluster_spawn.up().await;
    });

    // HTTP is separate from the gateway, so create a new client.
    let http = Arc::new(HttpClient::new(token.to_string()));

    let guilds = http
        .current_user_guilds()
        .limit(200)
        .unwrap()
        .await
        .unwrap()
        .model()
        .await
        .unwrap();
    dbg!(guilds);

    // Since we only care about new messages, make the cache only
    // cache new messages.
    let cache = InMemoryCache::builder()
        .resource_types(ResourceType::MESSAGE)
        .build();

    // Process each event as they come in.
    while let Some((shard_id, event)) = events.next().await {
        // Update the cache with the event.
        cache.update(&event);

        tokio::spawn(handle_event(shard_id, event, Arc::clone(&http)));
    }

    Ok(())
}

async fn handle_event(
    shard_id: u64,
    event: Event,
    http: Arc<HttpClient>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        Event::MessageCreate(msg) if msg.content == "!ping" => {
            http.create_message(msg.channel_id)
                .content("Pong!")?
                .await?;
        }
        Event::ShardConnected(_) => {
            println!("Connected on shard {shard_id}");
        }
        // Other events here...
        _ => {}
    }

    Ok(())
}
