use std::{
    cell::OnceCell,
    collections::{HashSet, VecDeque},
    fmt::Debug,
    hash::Hash,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU64, AtomicUsize},
        Arc, OnceLock,
    },
    time::Duration,
};

use culpa::throws;
use eyre::{bail, Context};
use fetcher::{depageinate_album, depageinate_playlist, depageinate_playlist_fast};
use futures::future::join_all;
use itertools::Itertools;
use kameo::{
    actor::{self, ActorRef, PreparedActor},
    error::BoxError,
    request::{MessageSend, TryMessageSend, TryMessageSendSync},
};
use parking_lot::{Mutex, RwLock};
use rspotify::{
    http::HttpError,
    model::{
        album, parse_uri, track, AlbumId, FullAlbum, FullPlaylist, FullTrack, Id, PlayableItem,
        PlaylistId, PlaylistTracksRef, TrackId, Type,
    },
    prelude::BaseClient,
    AuthCodeSpotify, ClientError, ClientResult,
};
use serenity::model::id;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Instant,
};
use tracing::{info, instrument};

use crate::{
    prelude::{Bug, Loggable},
    utils::when_even::OnError,
};

mod db;
mod fetcher;
mod init;

const MAX_ALBUMS: usize = 20;
const MAX_TRACKS: usize = 100;

pub enum ReqTypes {
    Album,
    Track,
    Playlist,
}

//TODO error on empty string
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub id: String,
    pub secret: String,
    pub redirect_url: String,

    pub token_cache_path: String,
}

pub struct Module {
    config: Config,
    client: Init<AuthCodeSpotify>,

    // TODO I was able to make queue not Arc, since I no longer spawn a task to consume them
    // This means I can also remove the mutexes from Queue implementation
    album_q: Queue<AlbumId<'static>>,
    track_q: Queue<TrackId<'static>>,

    ratelimiter: RateLimiter,

    // TODO I'd prefer this to be pulled in from some kind of tokio task local variable
    this: ActorRef<Module>,

    // TODO and I'd like to be able to have Tasks defined adhoc
    // Task.trigger(actor_ref), it would look up Task in the actor_ref's task list, which would be some kind of type map
    // Though there is an open question about Tasks with data.
    trigger: TriggerTask<Task, Self>,
}

pub async fn init_and_spawn(config: Config) -> ActorRef<Module> {
    kameo::actor::spawn_with(|actor_ref| async move {
        Module {
            config,
            client: Default::default(),
            this: actor_ref.clone(),
            album_q: Default::default(),
            track_q: Default::default(),
            ratelimiter: Default::default(),
            trigger: TriggerTask::new(actor_ref, Task),
        }
    })
    .await
}

impl kameo::Actor for Module {
    type Mailbox = kameo::mailbox::unbounded::UnboundedMailbox<Self>;

    #[throws(BoxError)]
    #[instrument(skip_all, err)]
    async fn on_start(&mut self, _: ActorRef<Self>) {
        // TODO there should be a pre start method allowing the creation of the Actor to be defered to after the creation of the ActorRef

        let client = init::connect(&self.config).await?;
        self.client.set(client);
        tracing::info!("SPOTIFY LOADED");

        // let q = self.album_q.clone();
        // let base = self.new_request(None);
        // tokio::spawn(async move {
        //     loop {
        //         let mut conn = base.new(None);
        //         let mut ids = q.wait_unclaimed(conn.reqid, 1, 20).await;
        //         conn.acquire().await; // get permit
        //         ids.extend(
        //             q.take_unclaimed(conn.reqid, 0, 20 - ids.len())
        //                 .unwrap_or_default(),
        //         );
        //         conn.albums(ids).await;
        //     }
        // });
    }

    // Implement other lifecycle hooks as needed...
}

#[kameo::messages]
impl Module {
    #[message]
    pub fn fetch_thing(&mut self, id: String) {
        match parse_uri(&id).context(id.clone()).unwrap().0 {
            Type::Album => {
                self.fetch_album(vec![id]);
            }
            Type::Track => {
                self.fetch_track(vec![id]);
            }
            Type::Playlist => {
                self.fetch_playlist(id);
            }
            a => tracing::warn!(id = id, "{}: not implemented", a),
        }
    }

    #[message]
    pub fn fetch_album(&mut self, ids: Vec<String>) {
        let ids = ids
            .into_iter()
            .map(|s| AlbumId::from_id_or_uri(&s).unwrap().clone_static());

        self.album_q.add_unique(ids);
        self.trigger.trigger_task();
    }

    #[message]
    pub fn fetch_track(&mut self, ids: Vec<String>) {
        let ids = ids
            .into_iter()
            .map(|s| TrackId::from_id_or_uri(&s).unwrap().clone_static());

        self.track_q.add_unique(ids);
        self.trigger.trigger_task();
    }

    #[message]
    pub fn fetch_playlist(&mut self, id: String) {
        // TODO may still want a request queue, so that we can decide priority
        dbg!(); // TODO what to log here? (universal log adaptor for actors??)
        let id = PlaylistId::from_id_or_uri(&id).unwrap().clone_static();
        tokio::spawn(self.new_request(None).playlist(id, None));
    }

    #[message(derive(Clone))]
    pub fn task(&mut self) {
        self.trigger.reset();
        while let Ok(c) = self.ratelimiter.connections.clone().try_acquire_owned() {
            // ALLOCATE A NEW CONNECTION
            match self.priotity() {
                Some(ReqTypes::Album) => {
                    let conn = self.new_request(Some(c));
                    let ids = self
                        .album_q
                        .take_unclaimed(conn.reqid, 1, MAX_ALBUMS)
                        .expect("priority wrong, nothing else should touch this");
                    tokio::spawn(conn.albums(ids));
                }
                Some(ReqTypes::Track) => {
                    let conn = self.new_request(Some(c));
                    let ids = self
                        .track_q
                        .take_unclaimed(conn.reqid, 1, MAX_TRACKS)
                        .expect("priority wrong, nothing else should touch this");
                    tokio::spawn(conn.tracks(ids));
                }
                None => {
                    tracing::info!("Idle");
                    break;
                }
                _ => {
                    todo!();
                }
            };
        }
    }

    pub fn priotity(&mut self) -> Option<ReqTypes> {
        // With this method how to prioritize?
        // reserve one connection for each request type
        // then prioritze playlists
        // then prioritze full batches
        // then prioritze
        if self.album_q.ready() > 0 {
            return Some(ReqTypes::Album);
        }
        if self.track_q.ready() > 0 {
            return Some(ReqTypes::Track);
        }
        None
    }

    /// get data back from fetcher
    #[message]
    async fn fetcher_data(&mut self, data: Vec<SpotifyThing>, reqid: Option<u64>) {
        // TODO handle missing tracks
        if let Some(reqid) = reqid {
            self.album_q.remove(reqid);
            self.track_q.remove(reqid); //XXX fixme
        }
        self.trigger.trigger_task();

        // TODO do something with the data
        for d in data {
            match d {
                SpotifyThing::Album(full_album) => {
                    tracing::trace!(name = &full_album.name);
                }
                SpotifyThing::Track(full_track) => {
                    tracing::trace!(name = &full_track.name);
                }
                SpotifyThing::Playlist(full_playlist) => {
                    tracing::trace!(name = &full_playlist.name);
                    // let mut v = Vec::new();
                    // for track in full_playlist.tracks.items {
                    //     let Some(track) = track.track else { continue };
                    //     let PlayableItem::Track(track) = track else {
                    //         continue;
                    //     };
                    //     let id = track.album.id.unwrap();
                    //     v.push(id.to_string());
                    // }
                    // self.fetch_album(v);
                }
            }
        }
    }

    /// get data back from fetcher
    #[message]
    fn fetcher_err(&mut self, err: ClientError, reqid: u64) {
        // TODO pass more request info, like kind
        match RateLimit::get(&err) {
            Some(rl) => {
                dbg!();
                self.album_q.release(reqid);
                self.track_q.release(reqid);
            }
            None => {
                tracing::error!("{}", err);
                self.album_q.remove(reqid);
                self.track_q.remove(reqid);
            }
        }
        self.trigger.trigger_task();
    }

    fn new_request(&self, c: Option<OwnedSemaphorePermit>) -> Conn {
        Conn {
            actor_ref: self.this.clone(),
            client: self.client.get().clone(),
            ratelimiter: self.ratelimiter.clone(),
            reqid: unique_id(),
            c,
        }
    }
}

struct Conn {
    actor_ref: ActorRef<Module>,
    client: AuthCodeSpotify,
    ratelimiter: RateLimiter,
    reqid: u64,
    c: Option<OwnedSemaphorePermit>,
}

impl Conn {
    #[tracing::instrument(skip_all)]
    async fn albums(mut self, ids: Vec<AlbumId<'static>>) {
        tracing::info!(count = ids.len());
        self.acquire().await;

        // TODO actually albums should cancel on rate limit.
        let mut albums = self
            .ratelimiter
            .with_rate_limit(
                || self.client.albums(ids.clone(), None),
                ids.len() == MAX_ALBUMS, //XXX moveme, abort on rate limit if this is a partial batch
            )
            .await
            .unwrap();
        self.c = None; // drop lease

        // depaginate tracks
        {
            // Really this should get splintered off into seperate sub connections
            for a in albums.iter_mut() {
                if a.tracks.total as usize > a.tracks.items.len() {
                    depageinate_album(&self.client, &self.ratelimiter, a)
                        .await
                        .unwrap();
                }
            }
        }

        let data = albums
            .into_iter()
            .map(|a| SpotifyThing::Album(a))
            .collect_vec();

        self.return_data(data).await;
    }

    #[tracing::instrument(skip_all)]
    async fn tracks(mut self, ids: Vec<TrackId<'static>>) {
        tracing::info!(count = ids.len());
        self.acquire().await;
        let tracks = self
            .ratelimiter
            .with_rate_limit(
                || self.client.tracks(ids.clone(), None),
                ids.len() == MAX_TRACKS,
            )
            .await
            .unwrap();
        self.c = None; // drop lease

        let data = tracks
            .into_iter()
            .map(|t| SpotifyThing::Track(t))
            .collect_vec();

        self.return_data(data).await;
    }

    #[tracing::instrument(skip_all)]
    async fn playlist(mut self, id: PlaylistId<'static>, snapshot: Option<String>) {
        self.acquire().await;
        let mut pl = self
            .ratelimiter
            .with_rate_limit(|| self.client.playlist(id.clone(), None, None), true)
            .await
            .unwrap();
        self.c = None; // drop lease

        if snapshot.as_ref() != Some(&pl.snapshot_id)
            && pl.tracks.total as usize > pl.tracks.items.len()
        {
            depageinate_playlist_fast(&self.client, &self.ratelimiter, &mut pl)
                .await
                .unwrap();
        }

        let data = SpotifyThing::Playlist(pl);
        self.return_data(vec![data]).await;
    }

    #[tracing::instrument(skip_all)]
    async fn playlist_sync(
        mut self,
        id: PlaylistId<'static>,
        snapshot: Option<String>,
        tracks: Vec<PlayableItem>,
    ) {
    }

    async fn acquire(&mut self) {
        if self.c.is_none() {
            let c = self
                .ratelimiter
                .connections
                .clone()
                .acquire_owned()
                .await
                .unwrap();
            self.c = Some(c);
        }
        assert!(self.c.is_some());
    }

    async fn return_data(self, data: Vec<SpotifyThing>) {
        self.actor_ref
            .tell(FetcherData {
                data,
                reqid: Some(self.reqid),
            })
            .send()
            .await
            .unwrap();
    }

    async fn send_error(self, err: ClientError) {
        self.actor_ref
            .tell(FetcherErr {
                err,
                reqid: self.reqid,
            })
            .send()
            .await
            .unwrap();
    }

    fn new(&self, c: Option<OwnedSemaphorePermit>) -> Conn {
        Self {
            actor_ref: self.actor_ref.clone(),
            client: self.client.clone(),
            ratelimiter: self.ratelimiter.clone(),
            reqid: unique_id(),
            c,
        }
    }
}

/// Data from the fetcher
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

#[derive_where::derive_where(Default)]
struct Queue<T> {
    data: RwLock<VecDeque<Req<T>>>,
    condvar: async_condvar_fair::Condvar,
}

impl<T: Clone + PartialEq + Debug + Hash + Eq> Queue<T> {
    fn add_unique(&self, v: impl IntoIterator<Item = T>) {
        let mut guard = self.data.write();
        let mut keys: HashSet<T> = guard.iter().map(|v| v.id.clone()).collect();

        let v = v
            .into_iter()
            .filter(|id| keys.insert(id.clone())) // clever way to ensure unique
            .map(|id| Req::new(id))
            .collect_vec();

        if !v.is_empty() {
            self.condvar.notify_all();
            guard.extend(v);
        }
    }

    pub fn take_unclaimed(&self, reqid: u64, min: usize, max: usize) -> Option<Vec<T>> {
        let mut guard = self.data.write();
        self._take_unclaimed(reqid, min, max, &mut guard)
    }

    fn _take_unclaimed(
        &self,
        reqid: u64,
        min: usize,
        max: usize,
        guard: &mut VecDeque<Req<T>>,
    ) -> Option<Vec<T>> {
        let free = guard
            .iter_mut()
            .filter(|r| r.req.is_none())
            .take(max)
            .collect_vec();

        if free.len() < min {
            return None;
        };

        let ids = free
            .into_iter()
            .map(|r| {
                r.req = Some(reqid);
                r.id.clone()
            })
            .collect_vec();

        Some(ids)
    }

    async fn wait_unclaimed(&self, reqid: u64, min: usize, max: usize) -> Vec<T> {
        loop {
            let mut data = self.data.write();
            let v = self._take_unclaimed(reqid, min, max, &mut data);
            if v.is_none() {
                self.condvar.wait_no_relock(data).await;
            };
        }
    }

    fn ready(&self) -> usize {
        let guard = self.data.read();
        guard.iter().filter(|r| r.req.is_none()).count()
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

    fn release(&self, reqid: u64) -> usize {
        let mut guard = self.data.write();
        let mut changed = 0;
        for r in guard.iter_mut() {
            if r.req == Some(reqid) {
                r.req = None;
                changed += 1;
            }
        }

        if changed > 0 {
            self.condvar.notify_all();
        }
        changed
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn unique_id() -> u64 {
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Clone)]
pub struct RateLimiter {
    sleep_until: Arc<Mutex<Option<Instant>>>,
    pub connections: Arc<tokio::sync::Semaphore>,
    pub revoked_count: Arc<AtomicUsize>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            connections: Semaphore::new(10).into(),
            sleep_until: Default::default(),
            revoked_count: Default::default(),
        }
    }
}

impl RateLimiter {
    pub async fn with_rate_limit_acquired<'async_trait, F, T>(
        &self,
        f: F,
    ) -> rspotify::ClientResult<T>
    where
        F: Fn() -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = rspotify::ClientResult<T>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >,
    {
        // let _c = match needs_aquired {
        //     true => Some(),
        //     false => None,
        // };
        let _c = self.connections.acquire().await;
        self.with_rate_limit(f, true).await
    }

    pub async fn with_rate_limit<'async_trait, F, T>(
        &self,
        f: F,
        retry: bool,
    ) -> rspotify::ClientResult<T>
    where
        F: Fn() -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = rspotify::ClientResult<T>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >,
    {
        let mut count = 0;
        const MAX_TRIES: i32 = 8;

        loop {
            loop {
                let t = self.sleep_until.lock().clone();
                match t {
                    Some(mut t) => {
                        let extra = (count - 1).max(1) as f32 / 2.0 + rand::random::<f32>();
                        t += Duration::from_secs_f32(extra);
                        tokio::time::sleep_until(t).await
                    }
                    None => {}
                };

                let mut guard = self.sleep_until.lock();

                // someone else might have extended the time
                let Some(t) = guard.as_ref() else { break };
                let elapsed = Instant::now().duration_since(*t);
                if elapsed != Duration::ZERO {
                    *guard = None;
                    break;
                }

                // tracing::info!(
                //     "lets see if this actually happens {}ms",
                //     elapsed.as_millis()
                // );
            }

            let v = f().await;
            let Some(rl) = RateLimit::get_res(&v) else {
                self.connections.add_permits(
                    self.revoked_count
                        .swap(0, std::sync::atomic::Ordering::Relaxed),
                );
                return v;
            };

            // RATE LIMIT HIT
            {
                // first gobble up all the connections
                let restore_permits = self.connections.forget_permits(100);
                let c = self
                    .revoked_count
                    .fetch_add(restore_permits, std::sync::atomic::Ordering::Relaxed);
                assert!(c + restore_permits < 10);
            }

            let n = rl.retry_after.unwrap_or(5.0);
            let n = n * 1.1f32.powi(count); // wait longer than spotify tells us too.

            let mut t = Instant::now() + Duration::from_secs_f32(n);

            {
                let mut guard = self.sleep_until.lock();
                t = guard.map(|t2| t.max(t2)).unwrap_or(t);
                *guard = Some(t);
            }

            tracing::warn!(RetryAfter = n, "RATE LIMIT [{}]", count);
            count += 1;

            if count >= MAX_TRIES || !retry {
                tracing::error!("RATE LIMIT [{}] abort", count);
                return v;
            }
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
#[derive_where::derive_where(Default)]
pub struct Init<T>(Option<T>);
impl<T> Deref for Init<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<T> Init<T> {
    pub fn set(&mut self, t: T) {
        assert!(self.0.is_none());
        self.0 = Some(t);
    }

    pub fn get(&self) -> &T {
        self.0.as_ref().expect("uninitialized")
    }
}

/// this represents a task that can be latched, only one copy of the Task will be on the actor's queue at a time
/// this solves three issues:
///     1. many different places can signal that the state machine needs a crank (we avoid complicated select! statements)
///     2. we can defer actually cranking the state machine untill after current messages are processed (as opposed to just calling a method on Actor)
///     3. we can avoid shared state; the state machine can take &mut (as opposed to tokio::spawn'd processing loop)
///
/// the handler for the Task must reset the latch, or it will never run again
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
    // TODO could we avoid having to instantiate this for dataless structs?
    // ie. Task::trigger() and it uses the current actor
    // where does the
    pub fn new(actor_ref: ActorRef<A>, task: T) -> Self {
        return Self {
            trigger: Default::default(),
            task,
            actor_ref,
        };
    }

    /// tell the actor to run the task, if it has not already been told
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

    /// called from task handler to clear the flag, allowing subsequent runs
    pub fn reset(&self) {
        self.trigger
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
