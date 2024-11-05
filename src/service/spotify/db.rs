use bevy_ecs::bundle::Bundle;
use eyre::ContextCompat;
use surrealdb::{
    opt::IntoQuery,
    sql::{self, statements::UpdateStatement, subquery},
    RecordId,
};
use tracing::instrument;
use tracing_subscriber::fmt::format;

use crate::prelude::*;

#[throws(eyre::Report)]
#[instrument(err, skip(db))]
pub async fn add_full_album(db: &MyDb, album: rspotify::model::FullAlbum) {
    let album_id = RecordId::from_table_key("album", &album.id.to_string());

    #[derive(Debug, Deserialize, Serialize)]
    struct ArtistBundle {
        id: RecordId,
        name: String,
    }

    let artist_bundle: Vec<ArtistBundle> = album
        .artists
        .iter()
        .filter_map(|a| {
            Some(ArtistBundle {
                id: RecordId::from(("artist", &a.id.clone()?.to_string())),
                name: a.name.clone(),
            })
        })
        .collect();
    let artists = artist_bundle.iter().map(|b| b.id.clone()).collect();

    #[derive(Debug, Deserialize, Serialize)]
    struct TrackBundle {
        id: RecordId,
        name: String,
        album: RecordId,
    }

    let track_bundle: Vec<TrackBundle> = album
        .tracks
        .items
        .iter()
        .filter_map(|a| {
            Some(TrackBundle {
                id: RecordId::from(("track", &a.id.clone()?.to_string())),
                name: a.name.clone(),
                album: album_id.clone(),
            })
        })
        .collect();
    let tracks = track_bundle.iter().map(|b| b.id.clone()).collect();

    #[derive(Debug, Deserialize, Serialize)]
    struct AlbumBundle {
        id: RecordId,
        artist: Vec<RecordId>,
        name: String,
        track: Vec<RecordId>,
        spotify_meta: rspotify::model::FullAlbum,
    }

    let bundle = AlbumBundle {
        id: album_id,
        artist: artists,
        name: album.name.clone(),
        track: tracks,
        spotify_meta: album,
    };

    //mvp is some way to bundle togeather
    // make this operate on list of FullAlbum

    let _res: Option<()> = db.update(bundle.id.clone()).content(bundle).await?;
}

#[throws(eyre::Report)]
#[instrument(err, skip(db))]
pub async fn add_full_track(db: &MyDb, track: rspotify::model::FullTrack) {
    #[derive(Debug, Deserialize, Serialize)]
    struct TrackBundle {
        id: RecordId,
        name: String,
        album: Option<RecordId>,
    }

    let id = track
        .id
        .context(format!("no TrackId for {})", &track.name))?
        .to_string();

    let album = track
        .album
        .id
        .map(|id| RecordId::from(("album".to_string(), id.to_string())));

    let bundle = TrackBundle {
        id: RecordId::from(("track".to_string(), id)),
        name: track.name,
        album,
    };

    let _res: Option<()> = db.update(bundle.id.clone()).content(bundle).await?;
}
