use std::{
    cell::OnceCell,
    sync::{Arc, OnceLock},
};

use culpa::throws;
use eyre::Error;
use futures::lock::Mutex;
use postage::sink::Sink;
use rustyline_async::ReadlineEvent;
use serde::{Deserialize, Serialize};
use surrealdb::{
    engine::any::{connect, Any},
    opt::auth::Root,
    sql::Table,
    Notification, RecordId, Surreal,
};
use tracing::info;

use crate::{
    types::{chat::Message, Link},
    utils::{
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
pub struct Database {
    pub db: MyDb,
    // sync
    // TODO MAINTAIN CACHE
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
    let d = Database { db: db.into() };

    d.link_task().await;

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
    }

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

    pub async fn link_task(&self) -> eyre::Result<()> {
        use futures::stream::StreamExt;
        let surreal = self.db.clone();
        tokio::spawn(async move {
            let mut stream = surreal
                .as_ref()
                .select("message")
                .live()
                .await
                .log::<OnError>()?;
            while let Some(result) = stream.next().await {
                handle(result);
            }
            Ok::<(), eyre::Report>(())
        });
        Ok(())
    }

    #[throws]
    pub async fn message_without_link(db: &MyDb) -> Vec<MessageBundle> {
        let ret: Vec<MessageBundle> = db
            .query("SELECT * FROM message WHERE link AND !->link")
            .await?
            .take(0)?;
        ret
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

#[derive(Debug, Serialize, Deserialize)]
struct MessageBundle {
    id: RecordId,
    message: Message,
}

fn handle(result: surrealdb::Result<Notification<MessageBundle>>) {
    match result {
        Ok(notification) => println!("{notification:?}"),
        Err(error) => eprintln!("{error}"),
    }
}

// #[cfg(test)]
// mod tests {
//     use super::Database;

//     async fn test_db() -> Database {
//         Database::init().await
//     }
// }

// usefull function for more than one id field
// combine with indexes for performance
//
// DEFINE FUNCTION fn::existential(
//     $table: string,
//     $index: string,
//     $value: any,
// ) {
// 	LET $id = SELECT
//         VALUE id
//         FROM type::table($table)
//         WHERE type::field($index) = $value;

//     return IF $id = [] THEN
//         return (
//             CREATE type::table($table) CONTENT object::from_entries([[$index, $value]])
//         ).id;
//     else
//         return $id;
//     end;
// };
