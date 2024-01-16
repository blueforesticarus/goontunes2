use std::{sync::Arc, thread};

use chrono::{DateTime, Utc};
use postage::sink::Sink;
use rustyline_async::ReadlineEvent;
use serde_json::json;
use surrealdb::{
    engine::local::{Db, Mem, RocksDb},
    sql::Thing,
    Surreal,
};
use tracing_subscriber::fmt::format;

use crate::utils::synctron::Synctron;

pub mod types {
    use chrono::{DateTime, Utc};
    use derivative::Derivative;
    use serde::{Deserialize, Serialize};
    use serde_with::{DeserializeFromStr, SerializeDisplay};
    use strum::{Display, EnumString};
    use surrealdb::sql::Thing;
    use url::Url;

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct Message {
        pub id: Thing,
        pub sender: Thing,
        pub channel: Thing,
        pub date: DateTime<Utc>,

        pub links: Vec<Link>,
    }

    #[derive(
        Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, DeserializeFromStr, SerializeDisplay,
    )]
    #[strum(ascii_case_insensitive)]
    #[strum(serialize_all = "lowercase")]
    pub enum MusicService {
        Spotify,
        Youtube,
        Soundcloud,
    }

    /// This data will actually be stuck into a relation
    #[derive(Derivative, Clone, Deserialize, Serialize)]
    #[derivative(Debug)]
    pub struct Link {
        pub service: MusicService,
        pub id: String,
        pub kind: Option<Kind>,

        #[derivative(Debug(format_with = "urlfmt"))]
        pub url: Url,

        pub target: Option<Thing>,
    }

    fn urlfmt(url: &Url, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "Url(\"{}\")", url.as_str())
    }

    #[derive(Debug, Copy, Clone, EnumString, Display, DeserializeFromStr, SerializeDisplay)]
    #[strum(ascii_case_insensitive)]
    #[strum(serialize_all = "lowercase")]
    pub enum Kind {
        Artist,
        Album,
        Track,
        Playlist,
        User,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct Track {
        pub id: Thing,
        pub service: Option<MusicService>,

        pub title: String,
        pub album: String,
        pub artist: Vec<String>,
    }

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct Album {
        pub id: Thing,
        pub service: Option<MusicService>,

        pub title: String,
        pub tracks: Vec<Thing>,
        pub artist: Vec<String>,
    }
}

#[derive(Debug, Clone)]
pub struct Database {
    db: Arc<Surreal<Db>>,

    // sync
    messages: Arc<Synctron>,
    tracks: Arc<Synctron>,
}

pub async fn init() -> Database {
    let db = Surreal::new::<RocksDb>(format!(
        "{}/temp.db",
        std::env::current_dir().unwrap().to_str().unwrap()
    ))
    .await
    .unwrap();
    db.use_ns("default").use_db("default").await.unwrap();
    Database {
        db: db.into(),
        messages: Default::default(),
        tracks: Default::default(),
    }
}

impl Database {
    pub async fn cmd_loop(&self) {
        use rustyline_async::Readline;
        use std::io::Write;
        let (mut rl, mut writer) = Readline::new(">> ".into()).unwrap();
        // if rl.load_history("history.txt").is_err() {
        //     println!("No previous history.");
        // }

        loop {
            let readline = rl.readline().await;
            match readline {
                Ok(ReadlineEvent::Line(line)) => {
                    rl.add_history_entry(line.clone());
                    let ret = self.db.query(line).await;
                    let ret = ret.map(|mut r| {
                        let j: Option<serde_json::Value> = r.take(0).unwrap();
                        j
                    });
                    writeln!(&mut writer, "{:#?}", ret);
                }
                Ok(ReadlineEvent::Interrupted) => {
                    println!("CTRL-C");
                    break;
                }
                Ok(ReadlineEvent::Eof) => {
                    println!("CTRL-D");
                    break;
                }
                Err(err) => {
                    println!("Error: {:?}", err);
                    break;
                }
            }
        }

        // #[cfg(feature = "with-file-history")]
        // rl.save_history("history.txt");
    }

    pub async fn add_message(&self, message: types::Message) -> eyre::Result<()> {
        let ret: Option<types::Message> =
            self.db.update(&message.id).content(message.clone()).await?;
        Ok(())
    }

    pub async fn _listen_message(&self) -> eyre::Result<()> {
        let ret = self
            .db
            .query("LIVE SELECT * FROM message WHERE ->link ")
            .await?;
        Ok(())
    }

    pub async fn most_recent(&self, channel: String) -> eyre::Result<Option<DateTime<Utc>>> {
        let ret: Option<DateTime<Utc>> = self
            .db
            .query("SELECT * FROM message WHERE channel == $channel ORDER BY date DESC LIMIT 1")
            .bind((
                "channel",
                Thing {
                    tb: "channel".into(),
                    id: channel.into(),
                },
            ))
            .await?
            .take("date")
            .unwrap();
        dbg!(ret);
        Ok(ret)
    }

    pub async fn add_track(&self, track: types::Track) -> eyre::Result<()> {
        let ret: Option<types::Track> = self.db.update(&track.id).content(track.clone()).await?;
        Ok(())
    }

    pub async fn add_album(&self, album: types::Album) -> eyre::Result<()> {
        let ret: Option<types::Track> = self.db.update(&album.id).content(album.clone()).await?;
        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use super::Database;

//     async fn test_db() -> Database {
//         Database::init().await
//     }
// }
