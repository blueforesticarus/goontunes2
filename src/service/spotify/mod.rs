use std::{
    collections::VecDeque,
    sync::{atomic::AtomicI32, Arc, OnceLock},
};

use culpa::throws;
use itertools::Itertools;
use kameo::{
    actor::{self, ActorRef},
    error::BoxError,
    request::MessageSend,
};
use parking_lot::{Mutex, RwLock};
use rspotify::{
    model::{AlbumId, FullAlbum, FullPlaylist, FullTrack, TrackId},
    prelude::BaseClient,
    AuthCodeSpotify,
};
use tokio::sync::Semaphore;

mod db;
//mod fetcher;
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
    pub client: OnceLock<AuthCodeSpotify>,

    pub album_q: Arc<Queue<AlbumId<'static>>>,
    pub track_q: Vec<TrackId<'static>>,

    pub connections: Arc<tokio::sync::Semaphore>,
}

impl kameo::Actor for Actor {
    type Mailbox = kameo::mailbox::unbounded::UnboundedMailbox<Self>;

    #[throws(BoxError)]
    async fn on_start(&mut self, actor_ref: ActorRef<Self>) {
        let client = init::connect(&self.config).await?;
        self.client.set(client).expect("should be unset");

        let q = self.album_q.clone();
        let client = self.client.get().unwrap().clone();
        let conns = self.connections.clone();

        tokio::spawn(async move {
            loop {
                {
                    let guard = q.data.read();
                    if guard.is_empty() {
                        q.condvar.wait_no_relock(guard);
                        continue;
                    }
                }

                let _conn = conns.acquire().await;

                let ids = {
                    let guard = q.data.read();
                    guard.iter().take(20).cloned().collect_vec()
                };

                let albums = client.albums(ids, None).await.unwrap();
                let data = albums
                    .into_iter()
                    .map(|a| (SpotifyThing::Album(a), ()))
                    .collect_vec();

                actor_ref.tell(FetcherData { data }).send().await;
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
//         _   ctx.actor_ref().tell(Spawn {}).send().await;
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

//         tokio::spawn(future)
//     }
// }

/// Data from the fetcher
type Metadata = ();
enum SpotifyThing {
    Album(FullAlbum),
    Track(FullTrack),
    Playlist(FullPlaylist),
}

struct Queue<T> {
    data: RwLock<VecDeque<T>>,
    condvar: async_condvar_fair::Condvar,
}

#[kameo::messages]
impl Actor {
    /// get data back from fetcher
    #[message]
    pub fn fetcher_data(&mut self, data: Vec<(SpotifyThing, Metadata)>) {}

    #[message]
    pub fn fetch_album(&mut self, ids: Vec<String>) {
        let mut guard = self.album_q.data.write();
        let current = guard.iter().map(|a| a.clone()).collect_vec();

        let mut filtered = ids
            .into_iter()
            .map(|s| AlbumId::from_id_or_uri(&s).unwrap().clone_static())
            .filter(|id| current.contains(id))
            .peekable();

        if filtered.peek().is_some() {
            self.album_q.condvar.notify_all();
            guard.extend(filtered);
        }
    }

    #[message]
    pub fn fetch_track(&mut self, ids: Vec<String>) {
        todo!();
    }

    #[message]
    pub fn task(&mut self, actor_ref: ActorRef<Actor>) {
        // if self.album_q.is_empty() {
        //     tokio::spawn(async { todo!() });
        // } else {
        //     let client = self.client.get().unwrap().clone();
        //     let ids = self.album_q.iter().take(20).cloned().collect_vec();
        //     tokio::spawn(async move {
        //         let albums = client.albums(ids, None).await.unwrap();
        //         let data = albums
        //             .into_iter()
        //             .map(|a| (SpotifyThing::Album(a), ()))
        //             .collect_vec();
        //         actor_ref.tell(FetcherData { data }).send().await;
        //         actor_ref.tell(Task).send().await;
        //     });
        // }
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
