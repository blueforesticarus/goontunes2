use std::{
    cell::OnceCell,
    sync::{Arc, OnceLock},
};

use culpa::throws;
use eyre::Error;
use futures::lock::Mutex;
use postage::{sink::Sink, stream::Stream};
use rustyline_async::{Readline, ReadlineEvent};
use serde::{Deserialize, Serialize};
use surrealdb::{
    engine::any::{connect, Any},
    opt::auth::Root,
    sql::Table,
    Action, Notification, RecordId, Surreal,
};
use tracing::info;

use crate::{
    types::{chat::Message, Link},
    utils::{
        links::extract_links,
        synctron::Synctron,
        when_even::{Loggable, OnError},
    },
};

static DATABASE: OnceLock<Database> = OnceLock::new();
pub type MyDb = Arc<Surreal<Any>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    path: String,
    auth: Option<Auth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auth {
    username: String,
    password: String,
}

#[derive(Debug, Clone)]
pub struct Gossip<T> {
    pub tx: postage::broadcast::Sender<T>,
}

impl<T: Clone> Default for Gossip<T> {
    fn default() -> Self {
        let (tx, _) = postage::broadcast::channel(100);
        Self { tx }
    }
}

impl<T: Clone> Gossip<T> {
    pub async fn publish(&self, v: T) {
        self.tx.clone().send(v).await;
    }

    pub async fn subscribe(&self) -> impl Stream<Item = T> {
        self.tx.subscribe()
    }

    pub async fn listen(&self, f: impl Fn(T)) {
        let mut a = self.tx.subscribe();
        loop {
            let r = a.recv().await.unwrap();
            f(r);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Database {
    pub db: MyDb,

    pub links: Gossip<Link>,
}

pub async fn init(config: Config) -> eyre::Result<Database> {
    info!("init database {}", &config.path);
    let db: MyDb = connect(config.path).await?.into();

    if let Some(auth) = config.auth {
        // Signin as a namespace, database, or root user
        db.signin(Root {
            username: &auth.username,
            password: &auth.password,
        })
        .await?;
    }

    db.use_ns("default").use_db("default").await.unwrap();
    let d = Database {
        db: db.into(),
        links: Default::default(),
    };

    Ok(d)
}

impl Database {
    pub async fn reset(&self, tables: Vec<&str>) {
        for table in tables {
            let res = self
                .db
                .query("delete $table")
                .bind(("table", Table::from(table)))
                .await;
            //dbg!(res);
        }
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

    #[throws]
    pub async fn update_link_edge(db: &MyDb, msg: RecordId, target: RecordId) {
        let query = "CREATE target; RELATE $msg->link->$target";
        db.query(query)
            .bind(("target", target))
            .bind(("msg", msg))
            .await?;
    }
}
