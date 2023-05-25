use std::{path::PathBuf, time::Duration};

use clap::{Parser, Subcommand, ValueEnum};
use eyre::{bail, Result};
use goontunes::{
    config::{Config, ServiceConfig},
    database::Database,
    service::{matrix::MatrixClient, spotify},
    traits::{ChatService, PlaylistService},
    types::CollectionId,
    utils::when_even::{with, WithContext},
};
use postage::stream::Stream;

#[derive(Debug, Clone, ValueEnum)]
enum GenerateKind {
    /// Save example config to config location
    Init,

    /// Print example config
    Print,
}
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
    generate: Option<GenerateKind>,

    /// Enable verbose logging output.
    #[clap(short, long, action)]
    verbose: bool,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
enum Commands {
    /// Fetch tracks for album or playlist
    GetTracks {
        /// Spotify uri
        id: String,
    },

    Start,
}

#[tokio::main]
async fn main() -> Result<()> {
    delegate().await
}

async fn delegate() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    //TODO write pull request https://gitlab.com/ijackson/rust-shellexpand/-/issues/8
    let path: PathBuf = shellexpand::path::tilde(&cli.config).into();

    if let Some(mode) = cli.generate {
        generate(mode, path)?;
        return Ok(());
    }

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

    match cli.command {
        Commands::GetTracks { id } => {
            // XXX different config for server and app
            let mut config: Vec<&spotify::client::Config> = config.get_service();
            let config = config.pop().expect("no spotify config").clone(); //XXX log config file location on error
            let client = config.init().await?;

            let collection = client
                .get_tracks(CollectionId {
                    service: goontunes::types::MusicService::Spotify,
                    id,
                })
                .await?;

            dbg!(collection);
        }
        Commands::Start => {
            init(config).await?;
        }
    }

    Ok(())
}

fn generate(mode: GenerateKind, path: PathBuf) -> Result<()> {
    let txt = serde_json::to_string_pretty(&Config::example())?;
    //actually config might not be good by design. this belongs in test if applicable
    //serde_json::from_str::<Config>(&txt)?; // make sure config is good.
    match mode {
        GenerateKind::Init => {
            std::fs::create_dir_all(path.parent().unwrap())?;
            if path.is_dir() {
                bail!("config path {} is dir", path.to_string_lossy())
            };
            if path.exists() {
                bail!("config path {} exists", path.to_string_lossy())
            }

            std::fs::write(&path, txt)?;
            println!("default config written to => {:?}", path);
        }
        GenerateKind::Print => {
            eprintln!("# {:?}", path);
            println!("{}", txt);
        }
    };

    Ok(())
}

async fn init(config: Config) -> Result<()> {
    let db1 = Database::init().await;

    for service_config in config.services.iter() {
        match service_config {
            ServiceConfig::Matrix(c) => {
                let db = db1.clone();
                let client = MatrixClient::connect(c.clone()).await?;

                dbg!("connected to matrix");
                let mut rx = client.channel();
                // do something with links
                tokio::spawn(async move {
                    dbg!("waiting for matrix messages");
                    loop {
                        tokio::select! {
                            Some(event) = rx.recv() => {
                                match event {
                                    goontunes::traits::ChatEvent::Message(msg) => {
                                        db.add_message(msg).await.unwrap();
                                    },
                                    goontunes::traits::ChatEvent::Reaction(react) => {
                                        db.add_reaction(react).await.unwrap();
                                    },
                                }
                            }
                        };
                    }
                });
            }
            ServiceConfig::Spotify(_) => todo!(),
        };
    }

    tokio::time::sleep(Duration::MAX).await; //TODO replace with shutdown logic

    // Exit success
    Ok(())
}
