use culpa::throws;
// While exploring, remove for prod.
use serde::{Deserialize, Serialize};
use surrealdb::dbs::Session;
use surrealdb::engine::local::Mem;
use surrealdb::kvs::Datastore;

use surrealdb::sql::Edges;
use surrealdb::Surreal;

type DB = (Datastore, Session);

#[throws(eyre::Report)]
#[tokio::main]
async fn main() {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("default").use_db("default").await?;

    #[derive(Debug, Serialize, Deserialize)]
    struct Test {
        name: String,
        meme: Edges,
    }
}
