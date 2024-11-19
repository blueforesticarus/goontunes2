use std::path::PathBuf;

use crate::prelude::*;

use rspotify::{clients::OAuthClient, scopes, AuthCodeSpotify, Credentials, OAuth};

#[throws(eyre::Error)]
#[tracing::instrument(skip_all)]
pub async fn connect(config: &super::Config) -> AuthCodeSpotify {
    let client = {
        let creds = Credentials::new(&config.id, &config.secret);
        let oauth = OAuth {
            redirect_uri: config.redirect_url.clone(),
            scopes: scopes!(
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
        let cache_path = PathBuf::from(shellexpand::full(&config.token_cache_path)?.to_string());

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
        tracing::info!(token = token.access_token);
        drop(guard); // Or else next request hangs

        let user = client.current_user().await.unwrap(); // XXX can fail on ratelimit
        tracing::info!(
            name = user.display_name,
            user = user.id.to_string(),
            "logged in"
        );
    }

    client
}
