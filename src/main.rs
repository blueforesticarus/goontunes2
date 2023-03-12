use std::{io::stderr, path::PathBuf};

use clap::{Parser, ValueEnum};
use eyre::{bail, Context, ContextCompat, Result};
use goontunes::{config::Config, service::matrix::MatrixClient, traits::ChatService};
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    //TODO write pull request https://gitlab.com/ijackson/rust-shellexpand/-/issues/8
    let path: PathBuf = shellexpand::path::tilde(&cli.config).into();

    if let Some(mode) = cli.generate {
        generate(mode, path)?;
        return Ok(());
    }

    let txt = std::fs::read_to_string(&path).context(path.to_string_lossy().to_string())?;
    let config: Config = serde_json::from_str(&txt)?;

    //TODO standardize
    if cli.verbose {
        tracing_subscriber::fmt::init();
    }

    init(config).await
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
    for service_config in config.services.iter() {
        match service_config {
            goontunes::config::ServiceConfig::Matrix(c) => {
                let mut client = MatrixClient::connect(c.clone()).await?;

                let mut mrx = client.message_channel();
                let mut rrx = client.react_channel();
                //TODO clone db handle

                // do something with links
                tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            Some(message) = mrx.recv() => {
                                println!("{} {:?}", message.sender.id, message.links[0].url)
                            },
                            Some(reaction) = rrx.recv() => {
                                println!("{} {:?}", reaction.sender.id, reaction.txt)
                            }
                        };
                    }
                });
            }
        };
    }

    // Exit success
    Ok(())
}
