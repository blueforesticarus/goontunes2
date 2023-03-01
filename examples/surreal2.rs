use serde::{Deserialize, Serialize};
use serde_json::json;
use surrealdb::{
    opt::{from_json, IntoQuery},
    sql::{Array, Datetime, Id, Object, Table, Thing, Value},
};
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

    let a = Value::Table(Table::from("test"));
    let a2 = json!(("asdf", a));
    dbg!(&a2);

    /*
    let link = Link {
        service: 0,
        url: Url::parse("http://google.com").unwrap(),
        id: "asdf".into(),
        kind: Some(0),
    };

    dbg!(&link);
    let j = json!(link);
    dbg!(&j);

    let v = from_json(j);
    dbg!(&v);
    */
}
