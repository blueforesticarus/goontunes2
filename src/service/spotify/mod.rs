use std::sync::{Arc, OnceLock};

use rspotify::AuthCodeSpotify;

use crate::prelude::MyDb;

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

pub struct Module {
    pub config: Config,
    pub client: OnceLock<AuthCodeSpotify>,
    pub db: MyDb,
    pub fetcher: fetcher::Fetcher,
}

impl Module {
    pub fn new(config: Config, db: MyDb) -> Arc<Self> {
        Self {
            config,
            db,
            client: OnceLock::new(),
            fetcher: Default::default(),
        }
        .into()
    }

    pub fn client(&self) -> &rspotify::AuthCodeSpotify {
        self.client.get().expect("spotify not initialized")
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
