use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use culpa::{throw, throws};
use deadqueue::unlimited::Queue;
use eyre::Error;
use rspotify::{
    clients::BaseClient,
    model::{album, AlbumId, FullAlbum, FullPlaylist, FullTrack, Page, PlaylistId, TrackId},
};
use serenity::async_trait;
use tokio::sync::Semaphore;
use tracing::{instrument, warn};

use crate::{types::Link, utils::when_even::Ignoreable};

/// requirements:
/// - fetch tracks for albums and albums for tracks
/// - be extensible to support fetching more metadata
/// - don't fetch things you already have
/// - dump everything to db
///
/// eventually what I should implement is a existential adapter with support for last update, errors, save/load from db, and limited cache.
/// and the real question is how to get the rspotify structs directly into the db with embeds replaced with links

// we just want to rate limit here.
// could use a library for rate limit bundling and caching. but that precludes interplay between different types of entities.
// downside is without a trait, can't abstract for database

// #[async_trait]
// trait Fetch<ID> {
//     type Value;
//     type Error = eyre::Report;
//     async fn get() -> Result<Self::Value, Self::Error>;
// }

//type Queue<T> = tokio::sync::Mutex<VecDeque<T>>;

struct Req<ID, V> {
    id: ID,
    tx: tokio::sync::oneshot::Sender<V>,
}

pub struct Fetcher {
    album_q: Queue<Req<AlbumId<'static>, FullAlbum>>,
    track_q: Queue<Req<TrackId<'static>, FullTrack>>,
    connections: tokio::sync::Semaphore,
}

impl Default for Fetcher {
    fn default() -> Self {
        Self {
            album_q: Default::default(),
            track_q: Default::default(),
            connections: Semaphore::new(10),
        }
    }
}

impl super::Module {
    #[throws]
    pub async fn album(&self, album: &str) -> FullAlbum {
        let id = AlbumId::from_id_or_uri(album)?.into_static(); //TODO impl try_into<AlbumId> for String
        let (tx, rx) = tokio::sync::oneshot::channel();

        // PROBLEM needs caching and such
        self.fetcher.album_q.push(Req { id, tx });
        let mut album = rx.await?;
        self.depageinate_album(&mut album).await?;
        album
    }

    #[throws]
    pub async fn track(&self, track: &str) -> FullTrack {
        let id = TrackId::from_id_or_uri(track)?.into_static();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.fetcher.track_q.push(Req { id, tx });
        rx.await?
    }

    #[throws]
    pub async fn playlist(&self, playlist: &str, snapshot: Option<&str>) -> FullPlaylist {
        let id = PlaylistId::from_id_or_uri(playlist)?.into_static();
        let mut res = self.client().playlist(id, None, None).await?;
        if snapshot != Some(&res.snapshot_id) {
            self.depageinate_playlist(&mut res).await?;
        }
        res
    }
}

// task to execute the fetching
impl super::Module {
    #[throws(eyre::Report)]
    async fn depageinate_album(&self, album: &mut FullAlbum) {
        let mut offset = album.tracks.offset;
        if album.tracks.next.is_some() {
            let _guard = self.fetcher.connections.acquire();
            while album.tracks.next.is_some() {
                let page = match self
                    .client()
                    .album_track_manual(album.id.clone(), None, None, Some(offset))
                    .await
                {
                    Ok(v) => v,
                    Err(e) => throw!(e),
                };

                if page.next.is_some() && page.limit != page.items.len() as u32 {
                    warn!("weird page {}", page.href);
                }

                album.tracks.next = page.next;
                album.tracks.items.extend(page.items.into_iter());
                offset += page.limit;
            }
        }
    }

    #[throws(eyre::Report)]
    async fn depageinate_playlist(&self, pl: &mut FullPlaylist) {
        let mut offset = pl.tracks.offset;
        if pl.tracks.next.is_some() {
            let _guard = self.fetcher.connections.acquire();
            while pl.tracks.next.is_some() {
                let page = match self
                    .client()
                    .playlist_items_manual(pl.id.clone(), None, None, None, Some(offset))
                    .await
                {
                    Ok(v) => v,
                    Err(e) => throw!(e),
                };

                if page.next.is_some() && page.limit != page.items.len() as u32 {
                    warn!("weird page {}", page.href);
                }

                pl.tracks.next = page.next;
                pl.tracks.items.extend(page.items.into_iter());
                offset += page.limit;
            }
        }
    }

    #[throws(eyre::Report)]
    #[instrument(err, skip(self))]
    async fn consume_album_q(&self) {
        let _guard = self.fetcher.connections.acquire();

        // get max 20 albums
        let mut ls = vec![self.fetcher.album_q.pop().await];
        while let Some(v) = self.fetcher.album_q.try_pop() {
            ls.push(v);
            if ls.len() >= 20 {
                break;
            }
        }

        let mut ls: HashMap<_, _> = ls.into_iter().map(|rq| (rq.id, rq.tx)).collect();

        let mut albums = self.client().albums(ls.keys().cloned(), None).await?;
        for album in albums.iter_mut() {
            //self.depageinate_album(&mut album);
            //add_full_album(&self.db, album.clone()).await.ignore();
            ls.remove(&album.id).unwrap().send(album.clone()).unwrap();
        }
    }

    #[throws(eyre::Report)]
    #[instrument(err, skip(self))]
    async fn consume_track_q(&self) {
        let _guard = self.fetcher.connections.acquire();

        let mut ls = vec![self.fetcher.track_q.pop().await];
        while let Some(v) = self.fetcher.track_q.try_pop() {
            ls.push(v);
            if ls.len() >= 100 {
                break;
            }
        }
        let mut ls: HashMap<_, _> = ls.into_iter().map(|rq| (rq.id, rq.tx)).collect();

        let tracks = self.client().tracks(ls.keys().cloned(), None).await?;
        for track in tracks {
            ls.remove(track.id.as_ref().unwrap())
                .unwrap()
                .send(track.clone())
                .unwrap();
        }
    }

    pub fn start_queue_task(self: Arc<Self>) {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                this.consume_album_q().await.ignore();
            }
        });

        tokio::spawn(async move {
            loop {
                self.consume_track_q().await.ignore();
            }
        });
    }
}

use super::db::*;

impl super::Module {
    pub async fn process_link(&self, link: Link) -> eyre::Result<()> {
        match link.kind.unwrap() {
            crate::types::Kind::Artist => todo!(),
            crate::types::Kind::Album => {
                let album = self.album(&link.id).await?;
                add_full_album(&self.db, album).await.unwrap();
            }
            crate::types::Kind::Track => {
                let track = self.track(&link.id).await?;
                add_full_track(&self.db, track).await.unwrap();
            }
            crate::types::Kind::Playlist => {
                todo!();
            }
            crate::types::Kind::User => todo!(),
        };

        Ok(())
    }
}
