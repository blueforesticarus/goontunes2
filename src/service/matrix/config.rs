use crate::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The homeserver to connect to.
    pub homeserver: Url,

    /// The user name that should be used for the login.
    pub username: String,

    /// The password that should be used for the login.
    pub auth: AuthConfig,

    /// TODO this needs to default to some kind of .cache/goontunes dir
    #[serde(default = "default_store_path")]
    pub matrix_crypto_store: String,

    #[serde(default = "default_store_pass")]
    pub matrix_crypto_pass: Option<String>,

    // Channels to listen to for tracks
    pub channels: Vec<matrix_sdk::ruma::OwnedRoomId>,
}

fn default_store_pass() -> Option<String> {
    Some("goontunes".to_string())
}

//NOTE: Must be function because https://github.com/serde-rs/serde/issues/2254
fn default_store_path() -> String {
    "~/.cache/goontunes/matrix/".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthConfig {
    Password(String), // TODO more auth methods
}

impl Config {
    pub fn example() -> Self {
        Self {
            homeserver: "https://matrix.org".try_into().unwrap(),
            username: "<username>".into(),
            auth: AuthConfig::Password("<password>".into()),
            matrix_crypto_store: default_store_path(),
            channels: vec!["!n8f893n9:example.com".try_into().unwrap()],
            matrix_crypto_pass: default_store_pass(),
        }
    }
}
