use serde::{Deserialize, Serialize};
use surrealdb::opt::IntoQuery;
use url::Url;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Link {
    pub service: u8,
    pub url: Url,
    pub id: String,
    pub kind: Option<u8>,
}

fn main() {
    let sql = "CREATE message CONTENT $data".into_query().unwrap();
    dbg!(sql);
}
