use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    future::Future,
    process::Output,
    sync::Arc,
};

use async_condvar_fair::Condvar;
use chrono::{format::Item, Local, Utc};
use culpa::{throw, throws};
use eyre::Error;
use futures::{future::join_all, stream::FuturesUnordered, FutureExt, Sink, SinkExt, StreamExt};
use itertools::Itertools;
use parking_lot::Mutex;
use rspotify::{
    clients::{pagination::paginate, BaseClient},
    model::{
        album, AlbumId, FullAlbum, FullPlaylist, FullTrack, ItemPositions, Page, PlayableId,
        PlayableItem, PlaylistId, PlaylistItem, SimplifiedTrack, TrackId,
    },
    prelude::OAuthClient,
    AuthCodeSpotify, DEFAULT_PAGINATION_CHUNKS,
};
use serenity::async_trait;
use tokio::sync::Semaphore;
use tracing::{instrument, warn, Instrument};

use crate::{
    prelude::Loggable,
    types::Link,
    utils::when_even::{Ignoreable, OnError},
};

use super::RateLimiter;

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

#[throws(eyre::Error)]
#[tracing::instrument("paginate", skip_all)]
pub async fn depageinate_album(
    client: &AuthCodeSpotify,
    ratelimiter: &RateLimiter,
    album: &mut FullAlbum,
) {
    if album.tracks.next.is_some() {
        let mut offset = album.tracks.offset;
        while album.tracks.next.is_some() {
            let page = match ratelimiter
                .with_rate_limit_acquired(|| {
                    client.album_track_manual(
                        album.id.clone(),
                        None,
                        Some(DEFAULT_PAGINATION_CHUNKS),
                        Some(offset),
                    )
                })
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

#[throws(eyre::Error)]
#[tracing::instrument(skip_all)]
pub async fn depageinate_album_fast<Fut>(
    client: &AuthCodeSpotify,
    ratelimiter: &RateLimiter,
    album: &mut FullAlbum,
    update: impl Fn(Page<SimplifiedTrack>) -> Fut,
) where
    Fut: Future<Output = ()>,
{
    // TODO feed data in real time
    if album.tracks.next.is_some() {
        let mut pages = vec![];
        let mut offset = album.tracks.offset;
        let mut ts = FuturesUnordered::new();
        while offset < album.tracks.total {
            // EDIT: refactored so lease is dropped as soon as connection is finished
            // let needs_lease = !ts.is_empty(); //first task reuses future from the client
            let id = album.id.clone();
            let f = ratelimiter.with_rate_limit_acquired(move || {
                client.album_track_manual(
                    id.clone(),
                    None,
                    Some(DEFAULT_PAGINATION_CHUNKS),
                    Some(offset),
                )
            });

            ts.push(f);
            offset += DEFAULT_PAGINATION_CHUNKS;
        }

        while let Some(page) = ts.next().await {
            let page = page?;
            update(page.clone()).await;
            pages.push(page);
        }

        // Fill in the page
        pages.sort_by_key(|f| f.offset);
        for page in pages {
            assert_eq!(album.tracks.items.len(), page.offset as usize);
            album.tracks.items.extend(page.items);
        }
    }
}

#[throws(eyre::Error)]
#[tracing::instrument(skip_all)]
pub async fn depageinate_playlist(
    client: &AuthCodeSpotify,
    ratelimiter: &RateLimiter,
    pl: &mut FullPlaylist,
) {
    if pl.tracks.next.is_some() {
        let mut offset = pl.tracks.offset;
        while pl.tracks.next.is_some() {
            let page = match ratelimiter
                .with_rate_limit_acquired(|| {
                    client.playlist_items_manual(
                        pl.id.clone(),
                        None,
                        None,
                        Some(DEFAULT_PAGINATION_CHUNKS),
                        Some(offset),
                    )
                })
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

#[throws(eyre::Error)]
#[tracing::instrument("paginate", skip_all)]
pub async fn depageinate_playlist_fast<Fut>(
    client: &AuthCodeSpotify,
    ratelimiter: &RateLimiter,
    pl: &mut FullPlaylist,
    // mut update: impl Sink<Page<PlaylistItem>, Error: Debug> + std::marker::Unpin,
    update: impl Fn(Page<PlaylistItem>) -> Fut,
) where
    Fut: Future<Output = ()>,
{
    // TODO feed data in real time
    if pl.tracks.next.is_some() {
        let mut pages = vec![];
        let mut offset = pl.tracks.offset + pl.tracks.items.len() as u32;
        let mut ts = FuturesUnordered::new();
        while offset < pl.tracks.total {
            // EDIT: refactored so lease is dropped as soon as connection is finished
            // let needs_lease = !ts.is_empty(); //first task reuses future from the client
            let id = pl.id.clone();
            let f = ratelimiter.with_rate_limit_acquired(move || {
                client.playlist_items_manual(
                    id.clone(),
                    None,
                    None,
                    Some(DEFAULT_PAGINATION_CHUNKS),
                    Some(offset),
                )
            });

            ts.push(f);
            offset += DEFAULT_PAGINATION_CHUNKS;
        }

        while let Some(page) = ts.next().await {
            let page = page?;
            // update.send(page.clone()).await.unwrap();
            update(page.clone()).await;
            pages.push(page);
        }

        // Fill in the page
        pages.sort_by_key(|f| f.offset);
        for page in pages {
            assert_eq!(pl.tracks.items.len(), page.offset as usize);
            pl.tracks.items.extend(page.items);
        }
    }
}

/// Does not support podcasts because I don't support Joe Rogan.
#[throws(eyre::Error)]
#[tracing::instrument(skip_all)]
async fn playlist_sync(
    client: &AuthCodeSpotify,
    ratelimiter: &RateLimiter,
    pl: FullPlaylist,
    target: Vec<TrackId<'static>>,
) {
    assert_eq!(
        pl.tracks.total as usize,
        pl.tracks.items.len(),
        "function expects a depaginated playlist"
    );

    let current: Result<Vec<_>, PlaylistItem> = pl
        .tracks
        .items
        .into_iter()
        .map(|t| -> Result<TrackId<'static>, PlaylistItem> {
            match t.track {
                Some(rspotify::model::PlayableItem::Track(FullTrack { id: Some(id), .. })) => {
                    Ok(id)
                }
                _ => Err(t),
            }
        })
        .try_collect();

    let actions = match current {
        Ok(current) => crate::utils::diff::sequence(current, target.clone(), Default::default()),
        Err(e) => {
            tracing::warn!(
                "Using full replacement due to weird stuff in playlist: {:?}",
                e
            );
            crate::utils::diff::full_replace(target.clone(), Default::default())
        }
    };

    for a in actions {
        match a {
            crate::utils::diff::Actions::Append(v) => {
                let items = v.into_iter().map(|t| t.into()).collect_vec();
                let foo = || client.playlist_add_items(pl.id.clone(), items.clone(), None);
                let ret = ratelimiter.with_rate_limit(foo, true).await;
                ret.log_and_drop::<OnError>();
            }
            crate::utils::diff::Actions::Add(v, i) => {
                let items = v.into_iter().map(|t| t.into()).collect_vec();
                let foo =
                    || client.playlist_add_items(pl.id.clone(), items.clone(), Some(i as u32));
                let ret = ratelimiter.with_rate_limit(foo, true).await;
                ret.log_and_drop::<OnError>();
            }
            crate::utils::diff::Actions::Delete(v) => {
                tracing::warn!("positional delete is broken");
                let mut hm = HashMap::<_, Vec<u32>>::new();
                for (i, t) in v {
                    hm.entry(t).or_insert(Default::default()).push(i as u32);
                }

                let foo = || {
                    let items = hm
                        .iter()
                        .map(|(id, p)| ItemPositions {
                            id: PlayableId::Track(id.clone_static()),
                            positions: &p,
                        })
                        .collect_vec();
                    client.playlist_remove_specific_occurrences_of_items(
                        pl.id.clone(),
                        items,
                        Some(&pl.snapshot_id),
                    )
                };
                let ret = ratelimiter.with_rate_limit(foo, true).await;
                ret.log_and_drop::<OnError>();
            }
            crate::utils::diff::Actions::DeleteAll(v) => {
                let items = v.into_iter().map(|t| t.into()).collect_vec();
                let foo = || {
                    client.playlist_remove_all_occurrences_of_items(
                        pl.id.clone(),
                        items.clone(),
                        Some(&pl.snapshot_id),
                    )
                };
                let ret = ratelimiter.with_rate_limit(foo, true).await;
                ret.log_and_drop::<OnError>();
            }
            crate::utils::diff::Actions::Replace(v) => {
                let items = v.into_iter().map(|t| t.into()).collect_vec();
                let foo = || client.playlist_replace_items(pl.id.clone(), items.clone());
                let ret = ratelimiter.with_rate_limit(foo, true).await;
                ret.log_and_drop::<OnError>();
            }
        }
    }

    // TODO rescan

    // update description
    let desc = pl.description.unwrap_or_default();
    let desc = desc.split("sync: ").next().unwrap();
    let mut desc = desc.trim().to_string();
    if !desc.is_empty() {
        desc += "\n\n";
    }
    let desc = format!("{desc}sync: {}", Local::now());

    let foo = || client.playlist_change_detail(pl.id.clone(), None, None, Some(&desc), None);
    let ret = ratelimiter.with_rate_limit(foo, true).await;
    ret.log_and_drop::<OnError>();
}
