use std::{path::PathBuf, time::Duration};

use clap::Parser;
use eyre::Result;
use goontunes::{
    config::{Config, ServiceConfig},
    database,
    service::{discord, matrix, spotify},
    utils::when_even::{with, WithContext},
};
use tracing::{info, trace, Level};
use tracing_subscriber::{
    filter::{self, Targets},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

#[derive(Debug, Clone, Parser)]
struct Cli {
    /// The homeserver to connect to.
    #[clap(
        short,
        long,
        env = "GOONTUNES_CONFIG",
        default_value = "~/.config/goontunes"
    )]
    config: String,

    #[clap(short, long)]
    reset: bool,

    /// Enable verbose logging output.
    #[clap(short, long, action)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    //TODO write pull request https://gitlab.com/ijackson/rust-shellexpand/-/issues/8
    let path: PathBuf = shellexpand::path::tilde(&cli.config).into();

    let config = with(&path)
        .with("serde deserialize config")
        .contextualize::<eyre::Error, _>(|| {
            let txt = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&txt)?;
            Ok(config)
        })?;

    //TODO standardize
    if cli.verbose {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(tracing_subscriber::EnvFilter::from_default_env())
            //.with(Targets::new().with_target("matrix_sdk_crypto", Level::WARN))
            .init();

        info!("initialized tracing");
    }

    let db = database::init(config.database).await.unwrap();
    if cli.reset {
        db.reset(vec!["message", "track", "album"]).await;
    }

    for service in config.services {
        match service {
            ServiceConfig::Discord(c) => {
                discord::Module::new(c, db.db.clone()).init().await.unwrap();
            }
            ServiceConfig::Matrix(c) => {
                matrix::Module::new(c, db.db.clone()).init().await.unwrap();
            }
            ServiceConfig::Spotify(c) => {
                spotify::Module::new(c, db.db.clone()).init().await.unwrap();
            }
        }
    }

    //db.cmd_loop().await;
    tokio::time::sleep(Duration::MAX).await;

    Ok(())
}
