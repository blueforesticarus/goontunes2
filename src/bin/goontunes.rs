#![feature(try_blocks)]
use std::{path::PathBuf, str::FromStr};

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use culpa::throws;
use derive_new::new;
use eyre::Result;
use goontunes::{
    config::{AppConfig, ConfigCli},
    service::{
        self,
        discord::{self, ScanSince},
        spotify::{self, FetchPlaylist, FetchThing, Init},
    },
    types::{self, chat::MessageBundle},
    utils::pubsub::PUBSUB,
};
use kameo::{actor::ActorRef, message::Message, Actor};
use serenity::all::ChannelId;
use tracing::{info, instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

#[derive(Debug, Clone, Parser)]
struct Cli {
    #[command(flatten)]
    config: ConfigCli,

    #[clap(short, long)]
    reset: bool,

    #[clap(long, value_enum)]
    generate_completions: Option<clap_complete::Shell>,

    /// disable tracing
    #[clap(short, long, action)]
    quiet: bool,

    /// Enable venator tracing consumer
    #[clap(long, action)]
    venator: bool,

    #[clap(long)]
    get: Vec<String>,
}

#[derive(new)]
struct CoreActor {
    #[new(default)]
    this: Init<ActorRef<Self>>,
    config: AppConfig,
    cli: Cli,

    #[new(default)]
    spotify: Option<ActorRef<spotify::Module>>,
    #[new(default)]
    discord: Option<ActorRef<discord::Module>>,
}

impl kameo::Actor for CoreActor {
    type Mailbox = kameo::mailbox::unbounded::UnboundedMailbox<Self>;

    #[throws(kameo::error::BoxError)]
    #[instrument(skip_all, err)]
    async fn on_start(&mut self, actor_ref: ActorRef<Self>) {
        self.this.set(actor_ref);

        // So the hazard of this method is that this must be set up FIRST
        // or else we could drop things
        PUBSUB
            .subscribe::<Vec<MessageBundle>, _>(self.this.get().clone())
            .await
            .unwrap();

        if let Some(conf) = self.config.spotify.get() {
            // TODO I don't like that it isn't a kameo function, wait for him to make prepare_with public
            self.spotify = Some(crate::service::spotify::init_and_spawn(conf.clone()).await);
        }

        if let Some(conf) = self.config.discord.get() {
            // TODO I don't like that it isn't a kameo function, wait for him to make prepare_with public
            self.discord = Some(crate::service::discord::init_and_spawn(conf.clone()).await);

            for channel_id in conf.channels.iter() {
                let channel_id = ChannelId::from_str(channel_id).unwrap();

                // TODO: this is not ergonomic
                let _ = self
                    .discord
                    .as_ref()
                    .unwrap()
                    .tell(ScanSince { channel_id })
                    .await
                    .unwrap();
            }
        }

        // TODO commands are simply actor messages
        for thing in self.cli.get.iter() {
            let thing = self
                .config
                .playlists
                .iter()
                .find(|pl| pl.name.as_ref().map(|s| s.to_lowercase()) == Some(thing.to_lowercase()))
                .map(|pl| pl.id.clone().unwrap())
                .unwrap_or(thing.clone());

            dbg!(&thing);
            let _: Option<()> = try {
                let _ = &self
                    .spotify
                    .as_ref()?
                    .tell(FetchPlaylist { id: thing.clone() })
                    .await
                    .unwrap();
            };
        }

        // for pl in self.config.playlists.iter() {
        //     if let Some(id) = &pl.id {
        //         dbg!(&id);
        //         let _ = actor_ref
        //             .tell(FetchPlaylist { id: id.clone() })
        //             .await
        //             .unwrap();
        //     }
        // }
    }
}

impl Message<Vec<MessageBundle>> for CoreActor {
    type Reply = ();

    async fn handle(
        &mut self,
        mut msg: Vec<MessageBundle>,
        _ctx: kameo::message::Context<'_, Self, Self::Reply>,
    ) -> Self::Reply {
        if msg.len() == 1 {
            tracing::info!("{:?}", msg);
        } else {
            tracing::info!("{} messages to process", msg.len());
        }

        msg.retain(|v| !v.links.is_empty());
        tracing::info!("{} with links", msg.len());

        for msg in msg {
            for link in msg.links {
                if link.service == types::Service::Spotify {
                    // Really this should be batched
                    if let Some(r) = &self.spotify {
                        r.tell(FetchThing { id: link.id }).await.unwrap();
                    } else {
                        // TODO latching messages
                        // tracing::info!("no_spotify")
                    }
                }
            }
        }
    }
}
fn print_completions<G: clap_complete::Generator>(gen: G, cmd: &mut clap::Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

#[tokio::main]
async fn main() -> Result<()> {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();

    if let Some(g) = cli.generate_completions {
        print_completions(g, &mut Cli::command());
        std::process::exit(0);
    }

    //TODO write pull request https://gitlab.com/ijackson/rust-shellexpand/-/issues/8
    let path: PathBuf = shellexpand::path::tilde(&cli.config.config_path).into();

    let config = goontunes::config::load(cli.config.clone()).unwrap();

    //TODO standardize
    tracing_subscriber::registry()
        .with(cli.venator.then(|| {
            venator::Venator::builder()
                .with_host("0.0.0.0:8362")
                .with_attribute("service", "goontunes")
                .with_attribute("environment", "dev")
                .build()
        }))
        .with((!cli.quiet).then(|| {
            tracing_subscriber::fmt::layer()
                .with_filter(tracing_subscriber::EnvFilter::from_default_env())
        }))
        // .with(cli.tokio_console.then(|| console_subscriber::init()))
        .init();

    info!("initialized tracing");

    kameo::actor::prepare(CoreActor::new(config, cli))
        .run()
        .await;

    Ok(())
}
