use crate::{
    traits::{ChatChannel, ChatEvent},
    types::{Reaction, Sender, SenderId},
    utils::{
        channel::Channel,
        links::{extract_urls, parse_url},
        takecell::TakeCell,
        when_even::{Ignoreable, Loggable, OnError},
    },
};
use chrono::{DateTime, NaiveDateTime, Utc};
use postage::sink::Sink;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{format, Display},
    io::Write,
    path::PathBuf,
    sync::Arc,
};

use clap::Parser;
use eyre::{bail, Context, ContextCompat, Result};
use futures::{future::join, pin_mut, stream::StreamExt};
use matrix_sdk::{
    config::{StoreConfig, SyncSettings},
    encryption::verification::{format_emojis, SasVerification, Verification},
    event_handler::Ctx,
    room::{Joined, MessagesOptions, Room},
    ruma::{
        events::{
            key::verification::{
                done::{OriginalSyncKeyVerificationDoneEvent, ToDeviceKeyVerificationDoneEvent},
                key::{OriginalSyncKeyVerificationKeyEvent, ToDeviceKeyVerificationKeyEvent},
                request::ToDeviceKeyVerificationRequestEvent,
                start::{OriginalSyncKeyVerificationStartEvent, ToDeviceKeyVerificationStartEvent},
            },
            reaction::{OriginalSyncReactionEvent, ReactionEventContent, Relation},
            room::{
                create::{RoomCreateEventContent, SyncRoomCreateEvent},
                member::StrippedRoomMemberEvent,
                message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
            },
            AnyMessageLikeEventContent, AnySyncMessageLikeEvent, AnySyncTimelineEvent,
            SyncMessageLikeEvent,
        },
        serde::Raw,
        EventId, OwnedEventId, OwnedRoomId, OwnedUserId, RoomId, UserId,
    },
    store::SledStateStore,
    Client,
};

use tokio::time::{sleep, Duration};
use url::Url;

use crate::{
    traits,
    types::{self, ChatService},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixConfig {
    /// The homeserver to connect to.
    pub homeserver: Url,

    /// The user name that should be used for the login.
    pub username: String,

    /// The password that should be used for the login.
    pub auth: MatrixAuth,

    /// TODO this needs to default to some kind of .cache/goontunes dir
    #[serde(default = "default_sled_path")]
    pub matrix_crypto_store: String,

    // Channels to listen to for tracks
    pub channels: Option<Vec<OwnedRoomId>>,
}

impl MatrixConfig {
    pub fn example() -> Self {
        Self {
            homeserver: "https://matrix.org".try_into().unwrap(),
            username: "<username>".into(),
            auth: MatrixAuth::Password("<password>".into()),
            matrix_crypto_store: default_sled_path(),
            channels: Some(vec!["!n8f893n9:example.com".try_into().unwrap()]),
        }
    }
}

//NOTE: Must be function because https://github.com/serde-rs/serde/issues/2254
fn default_sled_path() -> String {
    "~/.cache/goontunes/matrix/".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatrixAuth {
    Password(String), // TODO more auth methods
}

#[derive(Debug, Clone, Parser)]
pub enum MatrixCommands {
    Ping,
    Scan {
        #[arg(short, long)]
        all: bool,

        channel: Option<OwnedRoomId>,
    },
    History {
        channel: Option<OwnedRoomId>,
    },
    User {
        user_id: Option<OwnedUserId>,
    },
}

pub struct MatrixClient {
    client: Client,
    config: MatrixConfig,
    event_rx: TakeCell<<ChatChannel as Channel>::Receiver>,
    event_tx: tokio::sync::Mutex<<ChatChannel as Channel>::Sender>,
}

impl MatrixClient {
    pub async fn connect(config: MatrixConfig) -> Result<Arc<Self>> {
        // Create crypto store
        let mut home: PathBuf = shellexpand::full(&config.matrix_crypto_store)?
            .to_string()
            .try_into()?;

        std::fs::create_dir_all(&home)?; //TODO I don't like creating, .cache if it doesn't exist
        home.push(&config.username);

        let mut store_builder = SledStateStore::builder();
        store_builder.path(home.clone());
        store_builder.passphrase("passphrase".to_string());

        let state_store = store_builder.build()?;
        let crypto_store = state_store.open_crypto_store()?;
        use matrix_sdk_crypto::store::CryptoStore;

        // Check for existing device id (do manually so we can extract device id)
        let device = crypto_store
            .load_account()
            .await
            .context(format!(
                "matrix store corrupted, delete {:?} and redo verification",
                home
            ))?
            .map(|d| d.device_id().to_string());

        // Config client
        let store_config = StoreConfig::new()
            .state_store(state_store)
            .crypto_store(crypto_store);

        let client = {
            let builder = Client::builder()
                .homeserver_url(config.homeserver.clone())
                .handle_refresh_tokens()
                // TODO passphrase
                .store_config(store_config);

            builder.build().await?
        };

        // Config Login
        let mut login = match &config.auth {
            MatrixAuth::Password(password) => client.login_username(&config.username, password),
        };

        if let Some(device) = device.as_ref() {
            login = login.device_id(device);
        }

        let display_name = format!(
            "goontunes on {}",
            hostname::get()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|error| {
                    dbg!(error);
                    "UNKNOWN".to_string()
                })
        );

        // Actually login
        let response = login
            .initial_device_display_name(&display_name)
            .send()
            .await?;
        dbg!(response.device_id);

        // Wrapper Client
        let (event_tx, event_rx) = ChatChannel::new();

        let c = Arc::new(Self {
            client,
            config: config.clone(),
            event_rx: event_rx.into(),

            event_tx: event_tx.into(),
        });
        c.client.add_event_handler_context(c.clone());

        // handlers
        Self::install_verification_handlers(&c.client);
        Self::install_autojoin_handlers(&c.client);

        // An initial sync to set up state and so our bot doesn't respond to old
        // messages. If the `StateStore` finds saved state in the location given the
        // initial sync will be skipped in favor of loading state from the store
        let sync_token = c
            .client
            .sync_once(SyncSettings::default())
            .await
            .unwrap()
            .next_batch;

        // Method 1
        c.client.add_event_handler(Self::on_room_message);
        c.client.add_event_handler(Self::on_reaction);

        let settings = SyncSettings::default().token(sync_token);
        let c2 = c.clone();
        tokio::spawn(async move {
            c2.client
                .sync(settings)
                .await
                .expect("this should run forever");
        });

        Ok(c)
    }

    fn install_verification_handlers(client: &Client) {
        fn print_result(sas: &SasVerification) {
            let device = sas.other_device();

            println!(
                "Successfully verified device {} {} {:?}",
                device.user_id(),
                device.device_id(),
                device.local_trust_state()
            );
        }

        async fn print_devices(user_id: &UserId, client: &Client) {
            println!("Devices of user {}", user_id);

            for device in client
                .encryption()
                .get_user_devices(user_id)
                .await
                .unwrap()
                .devices()
            {
                println!(
                    "   {:<10} {:<30} {:<}",
                    device.device_id(),
                    device.display_name().unwrap_or("-"),
                    device.is_verified()
                );
            }
        }

        async fn wait_for_confirmation(client: Client, sas: SasVerification) {
            let emoji = sas.emoji().expect("The emoji should be available now");

            println!("\nDo the emojis match: \n{}", format_emojis(emoji));
            print!("Confirm with `yes` or cancel with `no`: ");
            std::io::stdout()
                .flush()
                .expect("We should be able to flush stdout");

            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .expect("error: unable to read user input");

            match input.trim().to_lowercase().as_ref() {
                "yes" | "true" | "ok" => {
                    sas.confirm().await.unwrap();

                    if sas.is_done() {
                        print_result(&sas);
                        print_devices(sas.other_device().user_id(), &client).await;
                    }
                }
                _ => sas.cancel().await.unwrap(),
            }
        }

        client.add_event_handler(
            |ev: ToDeviceKeyVerificationRequestEvent, client: Client| async move {
                let request = client
                    .encryption()
                    .get_verification_request(&ev.sender, &ev.content.transaction_id)
                    .await
                    .expect("Request object wasn't created");

                request
                    .accept()
                    .await
                    .expect("Can't accept verification request");
            },
        );

        client.add_event_handler(
            |ev: ToDeviceKeyVerificationStartEvent, client: Client| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                    .await
                {
                    println!(
                        "Starting verification with {} {}",
                        &sas.other_device().user_id(),
                        &sas.other_device().device_id()
                    );
                    print_devices(&ev.sender, &client).await;
                    sas.accept().await.unwrap();
                }
            },
        );

        client.add_event_handler(
            |ev: ToDeviceKeyVerificationKeyEvent, client: Client| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                    .await
                {
                    tokio::spawn(wait_for_confirmation(client, sas));
                }
            },
        );

        client.add_event_handler(
            |ev: ToDeviceKeyVerificationDoneEvent, client: Client| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.transaction_id.as_str())
                    .await
                {
                    if sas.is_done() {
                        print_result(&sas);
                        print_devices(&ev.sender, &client).await;
                    }
                }
            },
        );

        client.add_event_handler(
            |ev: OriginalSyncRoomMessageEvent, client: Client| async move {
                if let MessageType::VerificationRequest(_) = &ev.content.msgtype {
                    let request = client
                        .encryption()
                        .get_verification_request(&ev.sender, &ev.event_id)
                        .await
                        .expect("Request object wasn't created");

                    request
                        .accept()
                        .await
                        .expect("Can't accept verification request");
                }
            },
        );

        client.add_event_handler(
            |ev: OriginalSyncKeyVerificationStartEvent, client: Client| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                    .await
                {
                    println!(
                        "Starting verification with {} {}",
                        &sas.other_device().user_id(),
                        &sas.other_device().device_id()
                    );
                    print_devices(&ev.sender, &client).await;
                    sas.accept().await.unwrap();
                }
            },
        );

        client.add_event_handler(
            |ev: OriginalSyncKeyVerificationKeyEvent, client: Client| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                    .await
                {
                    tokio::spawn(wait_for_confirmation(client.clone(), sas));
                }
            },
        );

        client.add_event_handler(
            |ev: OriginalSyncKeyVerificationDoneEvent, client: Client| async move {
                if let Some(Verification::SasV1(sas)) = client
                    .encryption()
                    .get_verification(&ev.sender, ev.content.relates_to.event_id.as_str())
                    .await
                {
                    if sas.is_done() {
                        print_result(&sas);
                        print_devices(&ev.sender, &client).await;
                    }
                }
            },
        );
    }
    fn install_autojoin_handlers(client: &Client) {
        // respond to event from invited room preview (used to autojoin invites)
        async fn on_stripped_state_member(
            room_member: StrippedRoomMemberEvent,
            client: Client,
            room: Room,
        ) {
            if room_member.state_key != client.user_id().unwrap() {
                // the invite we've seen isn't for us, but for someone else. ignore
                return;
            }

            // looks like the room is an invited room, let's attempt to join then
            if let Room::Invited(room) = room {
                // The event handlers are called before the next sync begins, but
                // methods that change the state of a room (joining, leaving a room)
                // wait for the sync to return the new room state so we need to spawn
                // a new task for them.
                tokio::spawn(async move {
                    println!("Autojoining room {}", room.room_id());
                    let mut delay = 2;

                    while let Err(err) = room.accept_invitation().await {
                        // retry autojoin due to synapse sending invites, before the
                        // invited user can join for more information see
                        // https://github.com/matrix-org/synapse/issues/4345
                        eprintln!(
                            "Failed to join room {} ({err:?}), retrying in {delay}s",
                            room.room_id()
                        );

                        sleep(Duration::from_secs(delay)).await;
                        delay *= 2;

                        if delay > 3600 {
                            eprintln!("Can't join room {} ({err:?})", room.room_id());
                            break;
                        }
                    }
                    println!("Successfully joined room {}", room.room_id());
                });
            }
        }
        client.add_event_handler(on_stripped_state_member);
    }

    /// Realtime handler for messages
    async fn on_room_message(
        event: OriginalSyncRoomMessageEvent,
        room: Room,
        client: Ctx<Arc<MatrixClient>>,
    ) {
        // First, we need to unpack the message: We only want messages from rooms we are
        // still in and that are regular text messages - ignoring everything else.
        let Room::Joined(joined) = room else { return };
        match &event.content.msgtype {
            MessageType::Text(content) => {
                let content = content.body.trim();

                let username = joined
                    .client()
                    .account()
                    .get_display_name()
                    .await
                    .unwrap()
                    .unwrap();

                let cmd = content
                    .strip_prefix(format!("{}: ", username).as_str())
                    .or(content.strip_prefix('!'));

                //TODO respond to @ messages

                if let Some(content) = cmd {
                    // Use command handler
                    let mut cmd_txt: Vec<String> = content
                        .trim_start_matches('!') // permit @bot !command
                        .trim()
                        .split_ascii_whitespace()
                        .into_iter()
                        .map(ToString::to_string)
                        .collect();
                    cmd_txt.insert(0, "!".into());

                    let command = MatrixCommands::try_parse_from(cmd_txt);
                    match command {
                        Ok(command) => {
                            client.process_commands(command, joined, event).await;
                        }
                        Err(e) => {
                            let msg = RoomMessageEventContent::text_plain(e.render().to_string());
                            joined.send(msg, None).await.unwrap();
                        }
                    }
                } else {
                    // use regular message handler
                    client.process_message(event, joined.into()).await.unwrap();
                }
            }
            other => {
                dbg!(other);
            }
        }
    }

    /// Realtime handler for emoji reacts
    async fn on_reaction(
        event: OriginalSyncReactionEvent,
        room: Room,
        client: Ctx<Arc<MatrixClient>>,
    ) {
        client.process_reaction(event, room).await.unwrap();
    }

    /// delegate for !commands
    async fn process_commands(
        &self,
        command: MatrixCommands,
        room: Joined,
        event: OriginalSyncRoomMessageEvent,
    ) {
        //TODO, abstract beyond just matrix commands, allow cli as well
        let get_room = |channel: Option<OwnedRoomId>| match channel {
            Some(room_id) => self.client.get_room(&room_id),
            None => Some(room.clone().into()),
        };

        match command {
            MatrixCommands::Ping => {
                let content = RoomMessageEventContent::text_plain("pong");
                //Note: the last parameter is an optional transaction id
                room.send(content, None).await.unwrap(); //XXX
            }
            MatrixCommands::Scan { all, channel } => {
                let target: Room = get_room(channel).unwrap();
                //TODO handle non-existant room
                if all {
                    self.scan_room_and_ancestors(target, DateTime::<Utc>::MIN_UTC)
                        .await;
                } else {
                    self.scan_room_history(target, DateTime::<Utc>::MIN_UTC)
                        .await;
                }

                //TODO
            }
            MatrixCommands::User { user_id } => {
                let info = self.user_info(user_id.unwrap_or(event.sender)).await;
                let content = RoomMessageEventContent::text_plain(format!("{:#?}", info));
                room.send(content, None).await.unwrap();
            }
            MatrixCommands::History { channel } => {
                let target: Room = get_room(channel).unwrap();
                let history = RoomAncestry::get(&target).await;
                let content = RoomMessageEventContent::text_markdown(history.to_string());
                room.send(content, None).await.unwrap();
            }
        }
    }

    /// Process a (possibly new, possibly old) emoji react
    async fn process_reaction(&self, event: OriginalSyncReactionEvent, room: Room) -> Result<bool> {
        // TODO reprocess target to see if it has a url, so we don't hammer db for no reason
        // however, implement db to handle reactions to messages it doesn't know about so we can skip them
        // (also implement db to handle messages with no link)
        let event_id = event.content.relates_to.event_id;

        let data = Reaction {
            sender: types::SenderId {
                service: ChatService::Matrix,
                id: event.sender.to_string(),
            },
            target: types::MessageId(event_id.to_string()),
            date: event
                .origin_server_ts
                .to_system_time()
                .ok_or_else(|| eyre::eyre!("weird date {:?}", event.origin_server_ts))?
                .into(),
            id: types::ReactionId(event_id.to_string()),
            txt: vec![event.content.relates_to.key],
        };

        let t = room.event(&event_id).await?;
        let t: AnySyncTimelineEvent = t.event.deserialize()?.into();
        let AnySyncTimelineEvent::MessageLike(
            AnySyncMessageLikeEvent::RoomMessage(t)
        ) = t else { bail!("{:?}", t) };

        match t {
            SyncMessageLikeEvent::Original(t) => {
                if self.process_message(t, room).await? {
                    let mut guard = self.event_tx.lock().await;
                    guard.send(data.into()).await?;
                    return Ok(true);
                }
            }
            SyncMessageLikeEvent::Redacted(t) => {
                dbg!(t);
            }
        };

        Ok(false)
    }

    /// Process a (possibly new, possibly old) message
    async fn process_message(
        &self,
        event: OriginalSyncRoomMessageEvent,
        room: Room, /*TODO, should this really take room? */
    ) -> Result<bool> {
        //dbg!(&event.content);
        let links = extract_urls(event.content.body().to_owned());
        let links: Vec<types::Link> = links
            .into_iter()
            .filter_map(|url| match parse_url(url.clone()) {
                Some(link) => Some(link),
                None => {
                    //TODO, emit bad links to a bad link debug table (maybe do with tracing)
                    dbg!(url);
                    None
                }
            })
            .collect();

        if !links.is_empty() {
            let data = types::Message {
                id: types::MessageId(event.event_id.to_string()),
                channel: types::ChannelId {
                    service: ChatService::Matrix,
                    id: room.room_id().to_string(),
                },
                sender: types::SenderId {
                    service: ChatService::Matrix,
                    id: event.sender.to_string(),
                },
                date: event
                    .origin_server_ts
                    .to_system_time()
                    .ok_or_else(|| eyre::eyre!("weird date {:?}", event.origin_server_ts))?
                    .into(),
                links,
            };

            let mut guard = self.event_tx.lock().await;
            guard.send(data.into()).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn scan_room_and_ancestors(&self, target: Room, since: DateTime<Utc>) {
        let ancestry = RoomAncestry::get_try_join(target).await;
        println!("{}", ancestry);
        for target in ancestry.lineage.into_iter().filter_map(Result::ok).rev() {
            self.scan_room_history(target.clone(), since).await;

            let events = target
                .get_state_events_static::<RoomCreateEventContent>()
                .await;
            let event: eyre::Result<_> = try {
                events?
                    .get(0)
                    .unwrap()
                    .deserialize()?
                    .as_original()
                    .context("idk")?
                    .clone()
            };
            let Ok(event) = event.log::<OnError>() else {continue};
            let dt: DateTime<Utc> = event.origin_server_ts.to_system_time().unwrap().into();
            if dt > since {
                // this room was created after the since date, so previous room was ended before earliest messages we are scanning for
                break;
            }
        }
    }

    async fn scan_room_history(&self, target: Room, since: DateTime<Utc>) {
        println!("scanning {}", target.room_id());
        let mut opt = MessagesOptions::backward();
        opt.limit = 100.try_into().unwrap();

        //let s = vec!["*".to_string()];
        //opt.filter.types = Some(s.as_ref());
        let backward_stream = target.timeline_backward().await.unwrap();
        pin_mut!(backward_stream);

        while let Some(event) = backward_stream.next().await {
            let event = event.unwrap().event.deserialize().unwrap();

            if DateTime::<Utc>::from(event.origin_server_ts().to_system_time().unwrap()) < since {
                //too early
                break;
            }

            //TODO some kind of log
            if let AnySyncTimelineEvent::MessageLike(event) = event {
                match event {
                    AnySyncMessageLikeEvent::RoomMessage(SyncMessageLikeEvent::Original(event)) => {
                        self.process_message(event, target.clone())
                            .await
                            .log::<OnError>()
                            .ignore();
                    }
                    AnySyncMessageLikeEvent::Reaction(SyncMessageLikeEvent::Original(event)) => {
                        self.process_reaction(event, target.clone())
                            .await
                            .log::<OnError>()
                            .ignore();
                    }
                    _ => {}
                }
            }
        }
    }

    async fn get_room_try_join(&self, room_id: &RoomId) -> eyre::Result<Joined> {
        match self.client.get_joined_room(room_id) {
            Some(room) => Ok(room),
            None => {
                self.client
                    .join_room_by_id(room_id)
                    .await
                    .log::<OnError>()
                    .ignore();
                self.client
                    .get_joined_room(room_id)
                    .ok_or_else(|| eyre::Report::msg(format!("unknown room {room_id}")))
            }
        }
    }

    fn joined_room_ids(&self) -> Vec<OwnedRoomId> {
        self.client
            .joined_rooms()
            .into_iter()
            .map(|v| v.room_id().into())
            .collect()
    }

    async fn user_info(&self, user_id: OwnedUserId) -> Option<Sender> {
        let id = user_id.to_string();
        let mut alias: Vec<String> = Vec::new();
        let mut avatar: Option<Vec<u8>> = None;

        for room in self.client.joined_rooms() {
            if let Ok(Some(m)) = room.get_member(&user_id).await.log::<OnError>() {
                if let Some(n) = m.display_name() {
                    alias.push(n.to_string())
                }

                alias.push(m.name().to_string());
                //let avatar_url = m.avatar_url().map(|v| v.as_str().try_into().unwrap());
                if avatar.is_none() {
                    avatar = m
                        .avatar(matrix_sdk::media::MediaFormat::File)
                        .await
                        .log::<OnError>()
                        .unwrap_or(None)
                }

                //this break result in only geting aliases from at most one channel
                break;
            }
        }

        if alias.is_empty() {
            None
        } else {
            alias.sort();
            alias.dedup();
            Some(types::Sender {
                id: SenderId {
                    id,
                    service: ChatService::Matrix,
                },
                alias,
                avatar,
            })
        }
    }

    async fn joined(&self, room_id: &RoomId) -> Result<Joined> {
        self.client
            .get_joined_room(room_id)
            .wrap_err_with(|| format!("no joined room {}", room_id))
    }

    async fn read_receipt(&self, room_id: &RoomId, event_id: &EventId) -> Result<()> {
        self.joined(room_id).await?.read_receipt(event_id).await?;
        Ok(())
    }

    async fn typing(&self, room_id: &RoomId, on: bool) -> Result<()> {
        self.joined(room_id).await?.typing_notice(on).await?;
        Ok(())
    }

    async fn react_to(&self, room_id: &RoomId, event_id: &EventId, reaction: &str) -> Result<()> {
        let r = ReactionEventContent::new(Relation::new(event_id.into(), reaction.to_string()));
        let r = AnyMessageLikeEventContent::Reaction(r);

        self.joined(room_id).await?.send(r, None).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl traits::ChatService for MatrixClient {
    fn channel(&self) -> <ChatChannel as Channel>::Receiver {
        // XXX this is not so great and should perhaps be rewritten either with Result<Reciever> or more likely with some kind of RefCell
        self.event_rx.take().unwrap()
    }

    async fn rescan(&self, since: DateTime<Utc>) {
        let channels = self
            .config
            .channels
            .clone()
            .unwrap_or_else(|| self.joined_room_ids());

        for channel in channels.iter() {
            if let Ok(joined) = self.get_room_try_join(channel).await.log::<OnError>() {
                self.scan_room_and_ancestors(joined.into(), since).await;
            }
        }
    }

    async fn get_user_info(&self, user_id: String) -> Result<Option<Sender>> {
        let user_id = OwnedUserId::try_from(user_id)?;
        Ok(self.user_info(user_id).await)
    }
}

/// Used to get older (and newer) versions of the room
#[derive(Debug, Default, Clone)]
pub struct RoomAncestry {
    pub lineage: Vec<Result<Room, OwnedRoomId>>,
    pub offset: usize,
}

impl Display for RoomAncestry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg: Vec<String> = self
            .lineage
            .iter()
            .enumerate()
            .map(|(i, v)| match v {
                Ok(room) => {
                    if i != self.offset {
                        format!("- {}", room.room_id())
                    } else {
                        format!("- **{}**", room.room_id())
                    }
                }
                Err(roomid) => {
                    let txt = format!("- {} inaccessible", roomid);
                    match i {
                        0 => format!("- ...\n- {}", txt),
                        _ => format!("- {}\n- ...", txt),
                    }
                }
            })
            .collect();
        let msg = msg.join("\n");
        f.write_str(&msg).expect("when does this error?");
        Ok(())
    }
}

impl RoomAncestry {
    async fn get_try_join(room: Room) -> RoomAncestry {
        let client = room.client();
        let mut already_tried = Vec::new();

        loop {
            let ancestry = Self::get(&room).await;
            if !ancestry.is_complete() {
                if let Some(unjoined_room) = ancestry
                    .lineage
                    .iter()
                    .cloned()
                    .filter_map(Result::err)
                    .find(|r| !already_tried.contains(r))
                {
                    already_tried.push(unjoined_room.clone());
                    client
                        .join_room_by_id(&unjoined_room)
                        .await
                        .log::<OnError>()
                        .ignore();

                    // loop untill there are no unjoined rooms which we haven't tried to join.
                    // joining a room may cause new room to show up in ancestry
                    continue;
                }
            }

            return ancestry;
        }
    }

    async fn get(room: &Room) -> RoomAncestry {
        //! scan for descendants and ancestors of room,
        let mut ret = RoomAncestry::default();
        let client = room.client().clone();

        async fn get_previous(room: Room) -> Option<OwnedRoomId> {
            let created: Vec<Raw<SyncRoomCreateEvent>> =
                room.get_state_events_static().await.unwrap();
            let created = created.get(0).unwrap();
            let event = created.deserialize().unwrap();
            let event = event.as_original().unwrap();
            event
                .content
                .predecessor
                .clone()
                .map(|previous_room| previous_room.room_id)
        }

        let mut current = room.clone();
        while let Some(roomid) = get_previous(current).await {
            match client.get_room(&roomid) {
                Some(v) => {
                    current = v.clone();
                    ret.lineage.insert(0, Ok(v));
                }
                None => {
                    println!("cannot access room {}", roomid);
                    ret.lineage.push(Err(roomid));
                    break;
                }
            };
        }

        ret.offset = ret.lineage.len();
        ret.lineage.push(Ok(room.clone()));

        let mut current = room.clone();
        while let Some(tombstone) = current.tombstone() {
            let roomid = tombstone.replacement_room;
            match client.get_room(&roomid) {
                Some(v) => {
                    current = v.clone();
                    ret.lineage.push(Ok(v));
                }
                None => {
                    println!("cannot access room {}", roomid);
                    ret.lineage.push(Err(roomid));
                    break;
                }
            };
        }

        ret
    }

    fn is_complete(&self) -> bool {
        self.lineage.iter().any(|r| r.is_err())
    }
}
