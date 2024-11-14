use std::{
    cell::OnceCell,
    collections::{HashSet, VecDeque},
    ops::Deref,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU64},
        Arc, OnceLock,
    },
    time::Duration,
};

use culpa::throws;
use fetcher::depageinate_album;
use futures::future::join_all;
use itertools::Itertools;
use kameo::{
    actor::{self, ActorRef},
    error::BoxError,
    request::{MessageSend, TryMessageSend, TryMessageSendSync},
};
use parking_lot::{Mutex, RwLock};
use rspotify::{
    http::HttpError,
    model::{album, AlbumId, FullAlbum, FullPlaylist, FullTrack, TrackId},
    prelude::BaseClient,
    AuthCodeSpotify, ClientError, ClientResult,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Instant,
};

use crate::prelude::{Loggable, OnError};

mod db;
mod fetcher;
mod init;

//TODO error on empty string
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub id: String,
    pub secret: String,
    pub redirect_url: String,

    pub token_cache_path: String,
}

struct Actor {
    pub config: Config,
    pub client: Init<AuthCodeSpotify>,
    pub this: Init<ActorRef<Actor>>,

    pub album_q: Arc<Queue<AlbumId<'static>>>,
    pub track_q: Arc<Queue<TrackId<'static>>>,

    pub ratelimiter: RateLimiter,

    pub trigger: TriggerTask<Task, Self>,
}

impl kameo::Actor for Actor {
    type Mailbox = kameo::mailbox::unbounded::UnboundedMailbox<Self>;

    #[throws(BoxError)]
    async fn on_start(&mut self, actor_ref: ActorRef<Self>) {
        let client = init::connect(&self.config).await?;
        self.client.set(client);

        let q = self.album_q.clone();
        let client = self.client.get().clone();

        tokio::spawn(async move {
            loop {
                let req = unique_id();
                let ids = q.wait_unclaimed(req, 1, 2);

                //self.spawn_album_task(c);
            }
        });
    }

    // Implement other lifecycle hooks as needed...
}

#[derive(kameo::Actor)]
struct BatchFetcher {
    q: Vec<String>,
    conn: Arc<Semaphore>,
    current: i32,
}

// #[kameo::messages]
// impl BatchFetcher {
//     #[message]
//     pub async fn batch_add(&mut self, b: Vec<String>) {
//         self.q.extend(b.into_iter());
//         if todo!(){
//            _ctx.actor_ref().tell(Spawn {}).send().await;
//         }
//     }

//     #[message]
//     pub async fn spawn(&mut self, actor_ref: ActorRef<Actor>) {
//         let _conn = if self.current < 1 && self.q.len() > 1 {
//             match self.conn.try_acquire() {
//                 Ok(v) => v,
//                 _ => return,
//             }
//         } else if self.q.len() >= 20 {
//             self.conn.acquire().await.unwrap()
//         } else {
//             return;
//         };

//         //tokio::spawn(future)
//     }
// }

/// Data from the fetcher
type Metadata = ();
enum SpotifyThing {
    Album(FullAlbum),
    Track(FullTrack),
    Playlist(FullPlaylist),
}

#[derive(Debug, Clone)]
struct Req<T> {
    pub req: Option<u64>,
    pub id: T,
}

impl<T> Req<T> {
    fn new(value: T) -> Self {
        Req {
            req: None,
            id: value,
        }
    }
}

struct Queue<T> {
    data: RwLock<VecDeque<Req<T>>>,
    condvar: async_condvar_fair::Condvar,
}

impl<T: Clone + PartialEq> Queue<T> {
    fn add_unique(&self, v: impl IntoIterator<Item = T>) {
        let mut guard = self.data.write();
        let keys = guard.iter().map(|v| v.id.clone()).collect_vec();

        let v = v
            .into_iter()
            .filter(|id| !keys.contains(id))
            .map(|id| Req::new(id))
            .collect_vec();

        if v.is_empty() {
            self.condvar.notify_all();
            guard.extend(v);
        }
    }

    fn take_unclaimed(&self, n: usize, reqid: u64) -> Vec<T> {
        let mut guard = self.data.write();
        guard
            .iter_mut()
            .filter(|r| r.req.is_none())
            .take(n)
            .map(|r| {
                r.req = Some(reqid);
                r.id.clone()
            })
            .collect_vec()
    }

    async fn wait_unclaimed(&self, reqid: u64, min: usize, max: usize) -> Vec<T> {
        let mut v = Vec::new();
        while v.len() < min {
            self.wait();
            let a = self.take_unclaimed(max - v.len(), reqid);
            v.extend(a);
        }
        v
    }

    async fn wait(&self) {
        let guard = self.data.read();
        if guard.is_empty() {
            self.condvar.wait_no_relock(guard);
        }
    }

    fn remove(&self, reqid: u64) -> Vec<Req<T>> {
        let mut guard = self.data.write();
        let mut v = Vec::new();
        guard.retain(|r| {
            if r.req == Some(reqid) {
                v.push(r.clone());
                false
            } else {
                true
            }
        });
        v
    }

    fn release(&self, reqid: u64) {
        let mut guard = self.data.write();
        let mut changed = false;
        for r in guard.iter_mut() {
            if r.req == Some(reqid) {
                r.req = None;
                changed = true;
            }
        }

        if changed {
            self.condvar.notify_all();
        }
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn unique_id() -> u64 {
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[kameo::messages]
impl Actor {
    #[message]
    pub fn fetch_album(&mut self, ids: Vec<String>) {
        let ids = ids
            .into_iter()
            .map(|s| AlbumId::from_id_or_uri(&s).unwrap().clone_static());

        self.album_q.add_unique(ids);
    }

    #[message]
    pub fn fetch_track(&mut self, ids: Vec<String>) {
        todo!();
    }

    #[message]
    pub fn fetch_playlist(&mut self, id: String) {
        todo!();
    }

    #[message(derive(Clone))]
    pub fn task(&mut self) {
        self.trigger.reset();
        while let Ok(c) = self.ratelimiter.connections.clone().try_acquire_owned() {
            // ALLOCATE A NEW CONNECTION
            let reqid = unique_id();
            let ids = self.album_q.take_unclaimed(20, reqid);
            self.spawn_album_task(c, ids, reqid);

            // With this method how to prioritize?
        }

        // if self.album_q.is_empty() {
        //     tokio::spawn(async { todo!() });
        // } else {

        // }
    }

    pub fn spawn_album_task(
        &self,
        c: OwnedSemaphorePermit,
        ids: Vec<AlbumId<'static>>,
        reqid: u64,
    ) {
        let client = self.client.clone();
        let this = self.this.clone();
        let ratelimiter = self.ratelimiter.clone();
        tokio::spawn(async move {
            let mut albums = ratelimiter
                .with_rate_limit(|| client.albums(ids.clone(), None))
                .await
                .unwrap();

            // depaginate tracks
            {
                for a in albums.iter_mut() {
                    depageinate_album(&client, &ratelimiter, a).await.unwrap();
                }
            }

            let data = albums
                .into_iter()
                .map(|a| SpotifyThing::Album(a))
                .collect_vec();

            this.tell(FetcherData { data, reqid }).send().await.unwrap();
            drop(c);
        });
    }

    /// get data back from fetcher
    #[message]
    fn fetcher_data(&mut self, data: Vec<SpotifyThing>, reqid: u64) {
        self.album_q.remove(reqid);

        // TODO do something with the data
        todo!();
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    sleeping: Arc<Mutex<Option<Instant>>>,
    pub connections: Arc<tokio::sync::Semaphore>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            sleeping: Default::default(),
            connections: Semaphore::new(10).into(),
        }
    }
}

impl RateLimiter {
    pub async fn with_rate_limit<'async_trait, F, T>(&self, f: F) -> rspotify::ClientResult<T>
    where
        F: Fn() -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = rspotify::ClientResult<T>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >,
    {
        let mut restore_permits = 0;
        let mut count = 0;
        const MAX_TRIES: i32 = 10;

        loop {
            {
                let t = self.sleeping.lock().clone();
                match t {
                    Some(t) => tokio::time::sleep_until(t).await,
                    None => {}
                };
            }

            let v = f().await;
            let Some(rl) = RateLimit::get_res(&v) else {
                self.connections.add_permits(restore_permits);
                return v;
            };

            if count > MAX_TRIES {
                tracing::error!("RATE LIMIT [{}] abort", count);
                self.connections.add_permits(restore_permits);
                return v;
            }

            // RATE LIMIT HIT
            // first gobble up all the connections
            restore_permits = self.connections.forget_permits(100);

            let n = rl.retry_after.unwrap_or(5.0);
            let n = n * (1.2f32.powi(count as i32)); // wait longer than spotify tells us too.

            let mut t = Instant::now() + Duration::from_secs_f32(n);

            {
                let mut guard = self.sleeping.lock();
                t = guard.map(|t2| t.max(t2)).unwrap_or(t);
                *guard = Some(t);
            }

            tracing::warn!(RetryAfter = n, "RATE LIMIT [{}]", count);
            tokio::time::sleep_until(t).await;

            let mut guard = self.sleeping.lock();
            if let Some(t) = guard.as_ref() {
                if Instant::now().duration_since(*t) != Duration::ZERO {
                    *guard = None;
                }
            };

            count += 1;
        }
    }
}

struct RateLimit {
    retry_after: Option<f32>,
}

impl RateLimit {
    fn get(e: &ClientError) -> Option<Self> {
        match &e {
            ClientError::Http(e) => match e.as_ref() {
                HttpError::StatusCode(e) => {
                    if e.status().as_u16() == 429 {
                        let header = e.headers().get("Retry-After");
                        let n: Option<f32> = try { header?.to_str().ok()?.parse::<f32>().ok()? };
                        if n.is_none() {
                            dbg!(&e, header);
                        }
                        return Some(RateLimit { retry_after: n });
                    }
                }
                _ => {}
            },
            _ => {}
        }
        None
    }

    fn get_res<T>(e: &ClientResult<T>) -> Option<Self> {
        match e {
            Err(e) => Self::get(e),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Init<T>(Option<T>);
impl<T> Deref for Init<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
            .as_ref()
            .unwrap_or_else(|| panic!("unitialized thingy"))
    }
}

impl<T> Init<T> {
    fn set(&mut self, t: T) {
        assert!(self.0.is_none());
        self.0 = Some(t);
    }

    fn get(&self) -> &T {
        self.0.as_ref().expect("uninitialized")
    }
}

#[derive_where::derive_where(Clone)] // https://github.com/rust-lang/rust/issues/26925
struct TriggerTask<T: Clone, A: kameo::Actor> {
    trigger: Arc<AtomicBool>,
    task: T,
    actor_ref: ActorRef<A>,
}

impl<T, A> TriggerTask<T, A>
where
    T: Clone + Send + Sync + 'static,
    A: kameo::Actor<Mailbox = kameo::mailbox::unbounded::UnboundedMailbox<A>>
        + kameo::message::Message<T>,
{
    pub fn trigger_task(&self) {
        if !self
            .trigger
            .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            use kameo::request::TryMessageSendSync;
            self.actor_ref
                .tell(self.task.clone())
                .try_send_sync()
                .unwrap_or_else(|_| panic!("blar"));
        }
    }

    pub fn reset(&self) {
        self.trigger
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

// async fn playlist_sync(
//     &self,
//     mut current: Option<Collection>,
//     playlist: Collection,
// ) -> eyre::Result<()> {
//     let pid: PlaylistId = Uri::try_from(playlist.id.clone())
//         .unwrap()
//         .try_into()
//         .unwrap();
//     let pl = self.client.playlist(pid, None, None).await?;

//     if let Some(inner) = current.as_ref() {
//         if inner.snapshot.as_ref().unwrap() == &pl.snapshot_id {
//             current = None;
//         }
//     }

//     let current = if let Some(inner) = current {
//         inner
//     } else {
//         let tracks: ClientResult<Vec<PlaylistItem>> =
//             self.client.paginate(pl.tracks).try_collect().await;

//         let tracks = tracks?
//             .into_iter()
//             .map(|t| {
//                 let t = match t.track {
//                     Some(PlayableItem::Track(t)) => t,
//                     Some(_) => Err(PlaylistTrackError::NotTrack)?,
//                     None => Err(PlaylistTrackError::Missing)?,
//                 };

//                 Ok(t.id.expect("why no id").into())
//             })
//             .collect_vec();

//         Collection {
//             id: playlist.id.clone(),
//             kind: crate::types::Kind::Album,
//             name: pl.name,
//             tracks,
//             snapshot: Some(pl.snapshot_id),
//         }
//     };

//     let actions = sequence(current.tracks, playlist.tracks.clone(), Default::default());

//     for a in actions {
//         match a {
//             crate::utils::diff::Actions::Snapshot(_) => todo!(),
//             crate::utils::diff::Actions::Append(_) => todo!(),
//             crate::utils::diff::Actions::Add(_, _) => todo!(),
//             crate::utils::diff::Actions::Delete(_, _) => todo!(),
//             crate::utils::diff::Actions::DeleteAll(_, _) => todo!(),
//             crate::utils::diff::Actions::Replace(_) => todo!(),
//             crate::utils::diff::Actions::Move { index, count, to } => todo!(),
//         }
//     }

//     let final_tracks = self
//         .get_playlist(playlist.id)
//         .await?
//         .tracks
//         .into_iter()
//         .map(|v| v.map(|t| t.id))
//         .collect_vec();
//     if final_tracks != playlist.tracks {
//         bail!("final result is not correct")
//     }

//     Ok(())
// }
//}
