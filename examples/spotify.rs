use rspotify::{
    model::PlaylistId, prelude::*, scopes, AuthCodeSpotify, Config, Credentials, OAuth,
};

use futures::StreamExt;
use futures::TryStreamExt;

#[tokio::main]
async fn main() {
    // You can use any logger for debugging.
    env_logger::init();

    /*
    clientid: 1d68b91899e746fcab809f446472e27f
    clientsecret: 3dd9e31a841f490db4b1ed05a5a7cdf0
    redirect_uri: http://localhost:9091
     */

    // The credentials must be available in the environment. Enable the
    // `env-file` feature in order to read them from an `.env` file.
    let creds = Credentials {
        id: "1d68b91899e746fcab809f446472e27f".to_string(),
        secret: Some("3dd9e31a841f490db4b1ed05a5a7cdf0".to_string()),
    };

    // Using every possible scope
    let scopes = scopes!(
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
    );
    let oauth = OAuth {
        redirect_uri: "http://localhost:9091".to_string(),
        scopes,
        ..Default::default()
    };

    let mut spotify = AuthCodeSpotify::with_config(
        creds,
        oauth,
        Config {
            token_cached: true,
            token_refreshing: true,
            ..Default::default()
        },
    );

    let url = spotify.get_authorize_url(false).unwrap();
    // This function requires the `cli` feature enabled.
    spotify.prompt_for_token(&url).await.unwrap();

    let token = spotify.token.lock().await.unwrap();
    println!("Access token: {}", &token.as_ref().unwrap().access_token);
    println!(
        "Refresh token: {}",
        token.as_ref().unwrap().refresh_token.as_ref().unwrap()
    );
    drop(token);

    let playlist = spotify
        .playlist(
            &PlaylistId::from_uri("spotify:playlist:22rFrNdZ7fZuq8LhmkSjST").unwrap(),
            None,
            None,
        )
        .await
        .unwrap();

    dbg!(playlist.name, playlist.description);
    let stream = spotify.playlist_items(&playlist.id, None, None);
    stream
        .for_each(|t| async {
            match t {
                Ok(v) => match v.track {
                    Some(v) => match v {
                        rspotify::model::PlayableItem::Track(t) => {
                            println!("{} {} {}", t.album.name, t.artists[0].name, t.name)
                        }
                        rspotify::model::PlayableItem::Episode(_) => todo!(),
                    },
                    None => println!("Missing"),
                },
                Err(e) => {
                    println!("ERR {}", e);
                }
            }
        })
        .await;
}
