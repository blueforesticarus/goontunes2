use std::{path::PathBuf, time::Duration};

use clap::Parser;
use eyre::{bail, Result};
use goontunes::{
    config::{Config, ServiceConfig},
    database,
    service::discord,
    utils::when_even::{with, WithContext},
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

    /// Enable verbose logging output.
    #[clap(short, long, action)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    //TODO write pull request https://gitlab.com/ijackson/rust-shellexpand/-/issues/8
    let path: PathBuf = shellexpand::path::tilde(&cli.config).into();

    let config = with(&path)
        .with("serde deserialize config")
        .contextualize::<eyre::Error, _>(|| {
            let txt = std::fs::read_to_string(&path)?;
            let config: Config = serde_json::from_str(&txt)?;
            Ok(config)
        })?;

    //TODO standardize
    if cli.verbose {
        tracing_subscriber::fmt::init();
    }

    let db = database::init().await;

    for service in config.services {
        match service {
            ServiceConfig::Discord(c) => {
                discord::init(c, db.clone()).await;
            }
        }
    }

    //db.cmd_loop().await;
    tokio::time::sleep(Duration::MAX).await;

    Ok(())
}
