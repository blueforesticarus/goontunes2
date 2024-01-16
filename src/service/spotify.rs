use std::sync::Arc;

use futures::StreamExt;
use itertools::Itertools;
use rspotify::{
    model::{AlbumId, FullTrack, TrackId},
    prelude::BaseClient,
    AuthCodeSpotify,
};
use serenity::client;
use surrealdb::sql::Thing;

use crate::{
    database::Database,
    types::{self, Link, MusicService},
};

//TODO error on empty string
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub id: String,
    pub secret: String,
    pub redirect_url: String,

    pub token_cache_path: String,
}

#[derive(Clone)]
pub struct Client {
    pub config: Config,
    pub client: AuthCodeSpotify,
    pub db: Database,

    pub album_q: Arc<Queue<String>>,
    pub track_q: Arc<Queue<String>>,
}

impl Client {
    pub async fn init(config: Config, db: Database) -> eyre::Result<Client> {
        use rspotify::{prelude::OAuthClient, scopes, Credentials, OAuth};

        let client = {
            let creds = Credentials::new(&config.id, &config.secret);
            let oauth = OAuth {
                redirect_uri: config.redirect_url.clone(),
                scopes: scopes!(
                    "user-read-email",
                    "user-read-private",
                    "user-top-read",
                    "user-read-recently-played",
                    "user-follow-read",
                    "user-library-read",
                    "user-read-currently-playing",
                    "user-read-playback-state",
                    "user-read-playback-position",
                    "playlist-read-collaborative",
                    "playlist-read-private",
                    "user-follow-modify",
                    "user-library-modify",
                    "user-modify-playback-state",
                    "playlist-modify-public",
                    "playlist-modify-private",
                    "ugc-image-upload"
                ),
                ..Default::default()
            };

            //TODO there is a better crate for this
            //XXX unwrap
            let cache_path = shellexpand::full(&config.token_cache_path)
                .unwrap()
                .to_string()
                .try_into()
                .unwrap();

            let conf = rspotify::Config {
                cache_path,
                token_cached: true,
                token_refreshing: true,
                ..Default::default()
            };

            AuthCodeSpotify::with_config(creds, oauth, conf)
        };

        let url = client.get_authorize_url(false)?;
        // This function requires the `cli` feature enabled.
        tracing::info!("connecting to spotify");
        client.prompt_for_token(&url).await?; // TODO open server, and fallback to prompt

        {
            /* Test connection */
            let guard = client.token.lock().await.unwrap();
            let token = guard.as_ref().unwrap();
            tracing::info!(access_token = token.access_token);
            tracing::info!(refresh_token = token.access_token);
            drop(guard); // Or else next request hangs

            let user = client.current_user().await.unwrap();
            tracing::info!(
                name = user.display_name,
                user = user.id.to_string(),
                "logged in"
            );
        }

        Ok(Client {
            client,
            config: config.clone(),
            db,

            album_q: Default::default(),
            track_q: Default::default(),
        })
    }
}

impl Client {
    async fn process_link(&self, link: Link) -> eyre::Result<()> {
        match link.kind.unwrap() {
            crate::types::Kind::Artist => todo!(),
            crate::types::Kind::Album => {
                let aid = AlbumId::from_id_or_uri(&link.id).unwrap();
                let album = self.client.album(aid.clone(), None).await?;

                let helper = |t| async {
                    match t {
                        Ok(t) => Some(t),
                        Err(e) => {
                            dbg!(e);
                            None
                        }
                    }
                };

                let tracks: Vec<rspotify::model::SimplifiedTrack> = if album.tracks.next.is_none() {
                    album.tracks.items
                } else {
                    self.client
                        .album_track(aid, None)
                        .filter_map(helper)
                        .collect()
                        .await
                };

                for track in tracks.iter() {
                    let track = types::Track {
                        id: Thing {
                            tb: "track".into(),
                            id: track
                                .id
                                .clone()
                                .expect(&format!("{} {}", &album.name, &track.name))
                                .to_string()
                                .into(),
                        },
                        service: Some(MusicService::Spotify),
                        title: track.name.clone(),
                        album: album.name.clone(),
                        artist: track.artists.iter().map(|a| a.name.clone()).collect(),
                    };

                    self.db.add_track(track).await.unwrap();
                }

                let tracks = tracks
                    .into_iter()
                    .map(|t| Thing {
                        tb: "track".to_string(),
                        id: t.id.unwrap().to_string().into(),
                    })
                    .collect_vec();

                let album = types::Album {
                    id: Thing {
                        tb: "album".into(),
                        id: link.id.into(),
                    },
                    service: Some(MusicService::Spotify),
                    title: album.name,
                    artist: album.artists.into_iter().map(|a| a.name).collect(),
                    tracks,
                };
                self.db.add_album(album).await.unwrap();
            }
            crate::types::Kind::Track => {
                let sid = TrackId::from_id_or_uri(&link.id).unwrap();
                let track = self.client.track(sid, None).await?;

                let track = types::Track {
                    id: Thing {
                        tb: "track".into(),
                        id: link.id.into(),
                    },
                    service: Some(MusicService::Spotify),
                    title: track.name,
                    album: track.album.name,
                    artist: track.artists.into_iter().map(|a| a.name).collect(),
                };
                self.db.add_track(track).await.unwrap();
            }
            crate::types::Kind::Playlist => {
                dbg!(link);
            }
            crate::types::Kind::User => todo!(),
        };

        Ok(())
    }
}

use deadqueue::unlimited::Queue;

// mod cruft {
//     use crate::types;
//     use rspotify::{
//         clients::pagination::{paginate, paginate_with_ctx, Paginator},
//         model::{idtypes::*, Page},
//         AuthCodeSpotify, ClientResult,
//     };

//     impl TryFrom<types::CollectionId> for Uri<'static> {
//         type Error = IdError;

//         fn try_from(value: types::CollectionId) -> Result<Self, Self::Error> {
//             if value.service != types::MusicService::Spotify {
//                 return Err(IdError::InvalidPrefix);
//             }

//             Ok(Uri::from_uri(&value.id)?.into_static())
//         }
//     }

//     impl From<TrackId<'static>> for types::TrackId {
//         fn from(value: TrackId) -> Self {
//             Self {
//                 service: types::MusicService::Spotify,
//                 id: value.uri(),
//             }
//         }
//     }

//     impl TryFrom<types::Uri> for Uri<'static> {
//         type Error = IdError;

//         fn try_from(value: types::Uri) -> Result<Self, Self::Error> {
//             Uri::from_uri(&value.0).map(Uri::into_static)
//         }
//     }

//     impl<T: Id> From<T> for types::Uri {
//         fn from(value: T) -> Self {
//             Self(value.uri())
//         }
//     }
// }

// #[async_trait]
// impl PlaylistService for client::Client {
//     async fn get_playlist(&self, id: types::Uri) -> eyre::Result<Collection<Track>> {
//         let pid: PlaylistId = Uri::try_from(id.clone()).unwrap().try_into().unwrap();
//         let playlist = self.client.playlist(pid, None, None).await?;
//         let tracks: ClientResult<Vec<PlaylistItem>> =
//             self.client.paginate(playlist.tracks).try_collect().await;

//         let tracks = tracks?
//             .into_iter()
//             .map(|t| {
//                 let t = match t.track {
//                     Some(PlayableItem::Track(t)) => t,
//                     Some(_) => Err(PlaylistTrackError::NotTrack)?,
//                     None => Err(PlaylistTrackError::Missing)?,
//                 };

//                 Ok(Track {
//                     //metadata: None,
//                     name: t.name,
//                     id: t.id.expect("why no id").into(),
//                 })
//             })
//             .collect_vec();

//         Ok(Collection {
//             id,
//             kind: crate::types::Kind::Album,
//             name: playlist.name,
//             tracks,
//             snapshot: Some(playlist.snapshot_id),
//         })
//     }

//     async fn get_album(&self, id: types::Uri) -> eyre::Result<Collection<Track>> {
//         let aid: AlbumId = Uri::try_from(id.clone()).unwrap().try_into().unwrap();
//         let album = self.client.album(aid).await?;
//         let tracks: ClientResult<Vec<SimplifiedTrack>> =
//             self.client.paginate(album.tracks).try_collect().await;

//         // let tracks: ClientResult<Vec<SimplifiedTrack>> =
//         //     self.client.album_track(a).try_collect().await;

//         let tracks = tracks?
//             .into_iter()
//             .map(|t| Track {
//                 //metadata: None,
//                 name: t.name,
//                 id: TrackId::from(t.id.expect("why wouldn't there be a track id?")),
//             })
//             .map(Ok)
//             .collect_vec();

//         Ok(Collection {
//             id,
//             kind: crate::types::Kind::Album,
//             name: album.name,
//             tracks,
//             snapshot: None,
//         })
//     }

//     async fn get_track(&self, id: types::Uri) -> eyre::Result<Track> {
//         let tid: rspotify::model::TrackId = Uri::try_from(id.clone()).unwrap().try_into().unwrap();
//         let track = self.client.track(tid).await?;

//         Ok(Track {
//             //metadata: None,
//             name: track.name,
//             //album: track.album.name,
//             //artist: track.artists.name,
//             id: TrackId::from(track.id.expect("why wouldn't there be a track id?")),
//         })
//     }

//     async fn list_playlists(
//         &self,
//         user: Option<types::Uri>,
//     ) -> eyre::Result<Vec<types::PlaylistMeta>> {
//         let playlists: Vec<_> = if let Some(user) = user {
//             let uid: UserId = Uri::try_from(user.clone()).unwrap().try_into().unwrap();
//             self.client.user_playlists(uid)
//         } else {
//             self.client.current_user_playlists()
//         }
//         .collect()
//         .await;

//         let a: Vec<types::PlaylistMeta> = playlists
//             .into_iter()
//             .map(|r| {
//                 r.map(|p| types::PlaylistMeta {
//                     id: Some(p.id.into()),
//                     name: Some(p.name),
//                     owner: Some(p.owner.id.into()),
//                     snapshot: Some(p.snapshot_id),
//                 })
//             })
//             .try_collect()?;

//         Ok(a)
//     }

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
