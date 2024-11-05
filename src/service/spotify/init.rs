use std::path::PathBuf;

use crate::prelude::*;

use rspotify::{clients::OAuthClient, scopes, AuthCodeSpotify, Credentials, OAuth};

impl super::Module {
    #[throws(eyre::Report)]
    pub async fn init(self: Arc<Self>) {
        let config = &self.config;

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
            let cache_path =
                PathBuf::from(shellexpand::full(&config.token_cache_path)?.to_string());

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

        self.client.set(client).unwrap();
        self.start_queue_task();
    }
}
