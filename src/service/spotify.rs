use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use itertools::Itertools;
use rspotify::{
    model::SimplifiedTrack,
    prelude::{BaseClient, Id},
    ClientResult,
};

use crate::{
    traits::PlaylistService,
    types::{Collection, CollectionId, Track, TrackId},
};

use self::types::SpotifyTrackMetadata;

pub mod types {
    pub type SpotifyExtraInfo = String;
    pub struct SpotifyTrackMetadata {
        pub info: String,
        pub extra: SpotifyExtraInfo,
    }
}

pub mod client {
    use crate::traits::Example;
    use rspotify::AuthCodeSpotify;
    use std::path::PathBuf;

    //TODO error on empty string
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct Config {
        pub id: String,
        pub secret: String,
        pub redirect_url: String,

        pub token_cache_path: String,
    }

    impl Example for Config {
        fn example() -> Self {
            Self {
                id: "1d68b11899e746fcab709f446472e27f".into(),
                secret: "3dd9e32a841f490db4b1ed05b5a7cdf0".into(),
                redirect_url: "http://localhost:12996".to_string(),

                token_cache_path: rspotify::DEFAULT_CACHE_PATH.into(), //XXX local config, global dirs?
            }
        }
    }

    #[derive(Clone)]
    pub struct Client {
        pub config: Config,
        pub client: AuthCodeSpotify,
    }

    mod init {
        use super::*;
        use rspotify::{prelude::OAuthClient, scopes, AuthCodeSpotify, Credentials, OAuth};

        impl From<&Config> for OAuth {
            fn from(config: &Config) -> Self {
                Self {
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
                }
            }
        }

        impl From<&Config> for Credentials {
            fn from(config: &Config) -> Self {
                Credentials::new(&config.id, &config.secret)
            }
        }

        impl From<&Config> for rspotify::Config {
            fn from(config: &Config) -> Self {
                //TODO there is a better crate for this
                //XXX unwrap
                let cache_path = shellexpand::full(&config.token_cache_path)
                    .unwrap()
                    .to_string()
                    .try_into()
                    .unwrap();

                Self {
                    cache_path,
                    token_cached: true,
                    token_refreshing: true,
                    ..Default::default()
                }
            }
        }

        impl Config {
            pub async fn init(self) -> eyre::Result<Client> {
                let client =
                    AuthCodeSpotify::with_config((&self).into(), (&self).into(), (&self).into());

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
                    config: self.clone(),
                })
            }
        }
    }
}

mod cruft {
    use crate::types;
    use rspotify::{
        clients::pagination::{paginate, paginate_with_ctx, Paginator},
        model::{idtypes::*, Page},
        AuthCodeSpotify, ClientResult,
    };

    pub enum SpotifyIds<'a> {
        Album(AlbumId<'a>),
        Playlist(PlaylistId<'a>),
    }

    impl TryFrom<types::CollectionId> for SpotifyIds<'static> {
        type Error = IdError;

        fn try_from(value: types::CollectionId) -> Result<Self, Self::Error> {
            if value.service != types::MusicService::Spotify {
                return Err(IdError::InvalidPrefix);
            }
            let (kind, id) = parse_uri(&value.id)?;
            match kind {
                rspotify::model::Type::Album => {
                    AlbumId::from_id(id.to_string()).map(SpotifyIds::Album)
                }
                rspotify::model::Type::Playlist => {
                    PlaylistId::from_id(id.to_string()).map(SpotifyIds::Playlist)
                }
                _ => Err(IdError::InvalidType),
            }
        }
    }

    impl From<TrackId<'static>> for types::TrackId {
        fn from(value: TrackId) -> Self {
            Self {
                service: types::MusicService::Spotify,
                id: value.uri(),
            }
        }
    }
}

#[async_trait]
impl PlaylistService for client::Client {
    async fn get_tracks(&self, id: CollectionId) -> eyre::Result<Collection<Track>> {
        let spotify_id: cruft::SpotifyIds = id.clone().try_into().unwrap();
        match spotify_id {
            cruft::SpotifyIds::Album(a) => {
                let album = self.client.album(a.clone()).await?;
                let tracks: ClientResult<Vec<SimplifiedTrack>> =
                    self.client.paginate(album.tracks).try_collect().await;

                // let tracks: ClientResult<Vec<SimplifiedTrack>> =
                //     self.client.album_track(a).try_collect().await;

                let tracks = tracks?
                    .into_iter()
                    .map(|t| Track {
                        //metadata: None,
                        name: t.name,
                        id: TrackId::from(t.id.expect("why wouldn't there be a track id?")),
                    })
                    .collect_vec();

                Ok(Collection {
                    id,
                    kind: crate::types::Kind::Album,
                    name: album.name,
                    tracks,
                })
            }
            cruft::SpotifyIds::Playlist(a) => todo!(),
        }
    }
}
