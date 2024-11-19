use std::{collections::HashMap, sync::OnceLock, thread::JoinHandle};

use chrono::{Duration, DurationRound};
use db::MessageIdDatetimeBundle;
use futures::{stream::FuturesUnordered, Sink, Stream, StreamExt};
use itertools::Itertools;
use kameo::{actor::ActorRef, error::BoxError, messages, request::MessageSendSync};
use serenity::{
    all::{CacheHttp, ChannelId, Context, GetMessages, Message, MessageId},
    http::Http,
};
use tokio::sync::{Semaphore, SemaphorePermit};
use tracing::{instrument, Instrument};
use types::{chat::MessageBundle, Link};

use crate::{
    prelude::*,
    utils::{
        links::{self, extract_links},
        pubsub::PUBSUB,
    },
};

mod convert;
mod db;
mod init;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// discord bot token
    pub token: String,

    /// channels
    pub channels: Vec<String>,
}

pub async fn init_and_spawn(config: Config) -> ActorRef<Module> {
    kameo::actor::spawn_with(|actor_ref| async move {
        Module {
            config,
            client: Default::default(),
            this: actor_ref.clone(),
        }
    })
    .await
}

pub struct Module {
    config: Config,
    client: OnceLock<Context>,

    this: ActorRef<Self>,
}

// TODO think about, can I have a code organization technique where the same struct can be used as an Actor, or directly (for cli command for example)
impl kameo::Actor for Module {
    type Mailbox = kameo::mailbox::unbounded::UnboundedMailbox<Self>;

    #[throws(BoxError)]
    #[instrument(skip_all, err)]
    async fn on_start(&mut self, actor_ref: ActorRef<Self>) {
        let client = self::init::connect(&self.config, actor_ref.clone()).await?;
        self.client.set(client).expect("init");
    }
}

#[messages]
impl Module {
    #[message]
    pub async fn process_msg(&self, msgs: Vec<Message>) {
        for msg in msgs.iter() {
            tracing::trace!(author = msg.author.name, txt = msg.content);
        }

        let msgs = msgs.iter().map(MessageBundle::from).collect_vec();

        // This does nothing if there isn't a subscriber
        // XXX: Startup race condition?
        PUBSUB.publish(msgs).await.unwrap();
    }

    #[message]
    pub async fn scan_since(&self, channel_id: ChannelId) {
        //TODO will need some kind of busy flag
        let client = self.client.get().unwrap();

        // let last = get_last_message_for_channel(&self.db, channel_id)
        //     .await
        //     .log::<Bug>()
        //     .unwrap_or_default();
        // let last = last.map(|v| v.ts).unwrap_or_default();

        let actor_ref = self.this.clone();

        let fut = message_history_scan(channel_id, client.clone(), None, false, move |msgs| {
            actor_ref.tell(ProcessMsg { msgs }).send_sync().unwrap()
        });
        tokio::spawn(fut);

        // The worlds most beautiful state machine, reduced to a loop, because discord throttles simultaneous connections
        //tokio::spawn(parallel_message_history_scan(1, channel_id, client.clone()));
    }
}

#[instrument(skip_all, fields(channel = channel_id.get(), since=since.map(|s|s.get()), backward=backward))]
async fn message_history_scan<F>(
    channel_id: ChannelId,
    client: impl CacheHttp,
    since: Option<MessageId>,
    backward: bool,
    msg: F,
) -> Result<Vec<Message>, eyre::Error>
where
    F: Fn(Vec<Message>) -> (),
{
    let mut messages: Vec<Message> = Vec::new();
    let mut req = GetMessages::new().limit(100);
    if backward {
        if since.is_some() {
            req = req.before(since.unwrap());
        }
    } else {
        req = req.after(since.unwrap_or(MessageId::from(1))); // represents Discord_epoch, 0 panics
    }

    let mut progress = 0.0;
    let mut m_oldest: Option<DateTime> = None;

    loop {
        let ret = channel_id.messages(&client, req).await?;
        if ret.len() == 0 {
            break;
        }

        let oldest = ret.iter().min_by_key(|v| *v.timestamp).unwrap().id; //oldest
        let newest = ret.iter().max_by_key(|v| *v.timestamp).unwrap().id; //newest
        m_oldest.get_or_insert(*oldest.created_at());

        if backward {
            req = req.before(oldest);
            // progress = (Utc::now() - *oldest.created_at()).num_seconds() as f32;
            // progress /= (Utc::now() - m_oldest.unwrap()).num_seconds() as f32;
        } else {
            req = req.after(newest);
            progress = (*newest.created_at() - m_oldest.unwrap()).num_seconds() as f32;
            progress /= (Utc::now() - m_oldest.unwrap()).num_seconds() as f32;
        }

        tracing::info!(
            from = newest.created_at().to_string(),
            to = oldest.created_at().to_string(),
            progress,
            total = messages.len(),
            "got {} messages",
            ret.len()
        );

        msg(ret.clone()); // side channel for partial updates
        messages.extend(ret);
    }

    tracing::info!(total = messages.len(), "no more messages");
    Ok(messages)
}

// UNUSED BELOW
#[derive(Clone, Copy, Debug, PartialEq)]
struct FetcherLease {
    //
    begin: DateTime,

    //
    current: serenity::all::MessageId,

    //
    end: DateTime,

    // average minutes spaned by a request
    density: f32,

    // expected number of requests left in this fetcher
    remaining: f32,

    // actually an index
    id: usize,

    // needed for case where end is more recent than most recent message
    done: bool,
}

/// EDIT: this doesn't work, discord rate limits are too restrictive
/// Plan: fetch messages from long room history using n connections intelligently.
/// first fetch the oldest
/// then break the history up into up to n blocks, using average post density as a (conservative) heuristic
/// when a block finishes it releases it's connections, and subworkers can choose to subdivide,
/// priority of subdivision is based on estimated time to completion.
async fn parallel_message_history_scan(
    max: usize,
    channel_id: ChannelId,
    client: impl CacheHttp,
) -> Result<Vec<Message>, eyre::Error> {
    let foo = |f: FetcherLease| {
        let client = &client;
        async move {
            let span = tracing::info_span!("request");
            let ret = channel_id
                .messages(client, GetMessages::new().limit(100).after(f.current))
                .instrument(span)
                .await;
            (f.id, ret)
        }
    };

    let f = FetcherLease {
        begin: DateTime::UNIX_EPOCH,
        current: 1.into(),                    //serenity panics if zero
        end: Utc::now() + Duration::hours(1), //just to be safe
        density: 0.0,
        remaining: 0.0,
        done: false,
        id: 0,
    };

    let mut leases: Vec<FetcherLease> = vec![f];

    let mut futs = FuturesUnordered::new();
    futs.push(foo(f));

    let mut messages: HashMap<MessageId, Message> = Default::default();

    loop {
        let Some((id, ret)) = futs.next().await else {
            // all fetchers complete
            break;
        };
        let ret = ret?; // TODO retry on errors

        messages.extend(ret.iter().map(|m| (m.id.clone(), m.clone()))); //TODO there is a smarter way to do this. and really it should be done elsewhere
        tracing::info!(
            i = id,
            channel = channel_id.get(),
            after = leases[id].current.created_at().to_string(),
            num = messages.len(),
            "got messages"
        );

        if ret.len() == 0 {
            // we have hit the most recent message.
            leases[id].done = true;
            tracing::info!(i = id, "hit present");
        }

        if let Some(oldest) = ret.iter().min_by_key(|v| *v.timestamp) {
            if leases[id].current == MessageId::from(1) {
                // is initial request
                leases[id].current = oldest.id;
                //leases[id].begin = *oldest.timestamp;
            };

            let newest = ret.iter().max_by_key(|v| *v.timestamp).unwrap().id;
            let ts = *newest.created_at();

            if ts > leases[id].end {
                // this fetcher is done, others might still be running
                tracing::info!(i = id, "done");
                leases[id].done = true;
            } else {
                let span = ts - *leases[id].current.created_at();
                let span = span.num_minutes() as f32;
                leases[id].density += span;
                if leases[id].density != span {
                    // rolling average, skip on first
                    leases[id].density /= 2.0;
                }

                let remaining = (leases[id].end - ts);
                let remaining = remaining.num_minutes() as f32;
                leases[id].remaining = (remaining / leases[id].density);

                leases[id].current = newest;
            }
        }

        if !leases[id].done {
            // fetch the next thing
            futs.push(foo(leases[id]));
        }

        let active = leases.iter_mut().filter(|l| !l.done).count();

        // conn_sem: make sure theres enough connections for both the fut we just spawned plus another
        if active < max && active > 0 {
            dbg!(active, max);
            let Some(mut best) = leases
                .iter()
                .cloned()
                .filter(|f| f.remaining >= 2.0 && !f.done)
                .max_by_key(|f| f.remaining.round() as i32)
            else {
                continue;
            };

            let mid = best.end - (best.end - *best.current.created_at()) / 2;
            //dbg!(mid, best.end, *best.current.created_at());
            assert!(mid < best.end);
            assert!(mid > *best.current.created_at());

            let mut other = best;
            other.current = MessageId::from(make_snowflake(mid));
            other.begin = mid;
            other.density = 0.0; // reset this since we are jumping far away
            other.remaining = 0.0;
            other.id = leases.len();
            leases.push(other);
            futs.push(foo(other));

            // we are cutting it in half
            best.end = mid;
            best.remaining = 0.0; //setting this to zero ensures it won't get split until it gets a chance to run at least once
            leases[best.id] = best;
        }
    }

    Ok(messages.into_values().sorted_by_key(|m| m.id).collect_vec())
}

pub fn make_snowflake(dt: DateTime<Utc>) -> u64 {
    let a = (dt - DateTime::UNIX_EPOCH).num_milliseconds() as u64 - 1420070400000;
    a << 22 + 1 // serentity panics if it's zero

    // https://github.com/Not-Nik/rustflakes/blob/master/src/lib.rs
    //     SystemTime::now()
    //         .duration_since(UNIX_EPOCH)
    //         .expect("")
    //         .as_millis() as u64
    //         - 1420070400000
    // SnowflakeWorker::get_timestamp() << 22
    // | (self.worker_id & 31) << 17
    // | (self.process_id & 31) << 12
    // | ((self.increment - 1) & 0xFFF)
}
