use std::{path::PathBuf, time::Duration};

use clap::Parser;
use eyre::Result;
use goontunes::{
    config::{AppConfig, ConfigCli},
    database,
    service::{
        self, discord, matrix,
        spotify::{self, FetchPlaylist, FetchThing},
    },
    utils::when_even::{with, WithContext},
};
use kameo::request::MessageSend;
use tracing::{info, trace, Level};
use tracing_subscriber::{
    filter::{self, Targets},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

#[derive(Debug, Clone, Parser)]
struct Cli {
    #[command(flatten)]
    config: ConfigCli,

    #[clap(short, long)]
    reset: bool,

    /// Enable verbose logging output.
    #[clap(short, long, action)]
    verbose: bool,

    /// Enable verbose logging output.
    #[clap(long, action)]
    venator: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    //TODO write pull request https://gitlab.com/ijackson/rust-shellexpand/-/issues/8
    let path: PathBuf = shellexpand::path::tilde(&cli.config.config_path).into();

    let config = goontunes::config::load(cli.config).unwrap();

    //TODO standardize
    if cli.verbose {
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer())
            .with(tracing_subscriber::EnvFilter::from_default_env())
            //.with(Targets::new().with_target("matrix_sdk_crypto", Level::WARN))
            .init();

        info!("initialized tracing");
    }
    if cli.venator {
        // venator::Venator::default().install()
    }

    // let db = database::init(config.database).await.unwrap();
    // if cli.reset {
    //     db.reset(vec!["message", "track", "album"]).await;
    // }

    if let Some(conf) = config.spotify {
        let actor_ref = kameo::spawn(service::spotify::Actor::new(conf));
        for pl in config.playlists {
            if let Some(id) = pl.id {
                dbg!(&id);
                let _ = actor_ref.tell(FetchPlaylist { id }).send().await.unwrap();
            }
        }
    }

    tokio::time::sleep(Duration::MAX).await;

    Ok(())
}
