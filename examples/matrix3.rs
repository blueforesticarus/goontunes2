use std::{fmt::Display, io::Write, path::PathBuf, str::FromStr};

use goontunes::links::{extract_urls, parse_url, Kind};

use clap::Parser;
use eyre::Result;
use futures::{pin_mut, stream::StreamExt};
use matrix_sdk::{
    config::SyncSettings,
    encryption::verification::{format_emojis, Emoji, SasVerification, Verification},
    room::{MessagesOptions, Room},
    ruma::{
        api::client::{
            device::get_device::v3::Response,
            filter::RoomEventFilter,
            filter::{FilterDefinition, LazyLoadOptions},
            message::get_message_events::v3::Direction,
        },
        events::{
            key::verification::{
                done::{OriginalSyncKeyVerificationDoneEvent, ToDeviceKeyVerificationDoneEvent},
                key::{OriginalSyncKeyVerificationKeyEvent, ToDeviceKeyVerificationKeyEvent},
                request::ToDeviceKeyVerificationRequestEvent,
                start::{OriginalSyncKeyVerificationStartEvent, ToDeviceKeyVerificationStartEvent},
            },
            reaction::{OriginalSyncReactionEvent, SyncReactionEvent},
            room::{
                self,
                create::{RoomCreateEvent, SyncRoomCreateEvent},
                member::StrippedRoomMemberEvent,
                message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
            },
            AnySyncMessageLikeEvent, AnySyncTimelineEvent, AnyTimelineEvent, StateEventType,
            SyncMessageLikeEvent,
        },
        serde::Raw,
        OwnedRoomId, OwnedUserId, RoomId, UserId,
    },
    Client, LoopCtrl,
};
use tokio::time::{sleep, Duration};
use url::Url;

fn install_verification_handlers(client: Client) {
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

async fn sync(client: Client) -> matrix_sdk::Result<()> {
    install_verification_handlers(client.clone());

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

    // This fn is called whenever we see a new room message event. You notice that
    // the difference between this and the other function that we've given to the
    // handler lies only in their input parameters. However, that is enough for the
    // rust-sdk to figure out which one to call one and only do so, when
    // the parameters are available.
    async fn on_room_message(event: OriginalSyncRoomMessageEvent, room: Room) {
        // First, we need to unpack the message: We only want messages from rooms we are
        // still in and that are regular text messages - ignoring everything else.
        dbg!(&event.content.msgtype);
        let Room::Joined(joined) = room else { return };
        match event.content.msgtype {
            MessageType::Text(text_content) => {
                // here comes the actual "logic": when the bot see's a `!party` in the message,
                // it responds
                let parts: Vec<String> = text_content
                    .body
                    .split_whitespace()
                    .map(|a| a.to_string())
                    .collect();
                let command = parts.get(0).map(String::as_str).unwrap_or("");
                let room_to_scan = if let Some(roomid) = parts.get(1) {
                    let roomid = match OwnedRoomId::from_str(roomid) {
                        Ok(v) => v,
                        Err(e) => {
                            return;
                        }
                    };
                    match joined.clone().client().get_room(&roomid) {
                        Some(r) => r,
                        None => {
                            dbg!("room does not exist", roomid);
                            return;
                        }
                    }
                } else {
                    joined.clone().into()
                };

                match command {
                    "!party" => {
                        let content =
                            RoomMessageEventContent::text_plain("🎉🎊🥳 let's PARTY!! 🥳🎊🎉");
                        println!("sending");
                        // send our message to the room we found the "!party" command in
                        // the last parameter is an optional transaction id which we don't
                        // care about.
                        joined.send(content, None).await.unwrap();
                        println!("message sent");
                    }
                    "!scan" => {
                        scan_room_history(room_to_scan).await;
                    }
                    "!scan_all" => {
                        let ancestry = RoomAncestry::get(room_to_scan).await;
                        for room_to_scan in
                            ancestry.lineage.into_iter().filter_map(Result::ok).rev()
                        {
                            scan_room_history(room_to_scan).await;
                        }
                    }
                    "!history" => {
                        let ancestry = RoomAncestry::get(room_to_scan).await;
                        let msg: Vec<String> = ancestry
                            .lineage
                            .iter()
                            .enumerate()
                            .map(|(i, v)| match v {
                                Ok(room) => {
                                    if i != ancestry.offset {
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
                        let content = RoomMessageEventContent::text_markdown(msg);
                        joined.send(content, None).await.unwrap();
                    }
                    _ => {}
                }
            }
            other => {
                dbg!(other);
            }
        }
    }

    // An initial sync to set up state and so our bot doesn't respond to old
    // messages. If the `StateStore` finds saved state in the location given the
    // initial sync will be skipped in favor of loading state from the store
    let sync_token = client
        .sync_once(SyncSettings::default())
        .await
        .unwrap()
        .next_batch;

    client.add_event_handler(on_room_message);
    client.add_event_handler(on_reaction);

    let settings = SyncSettings::default().token(sync_token);
    client.sync(settings).await?;

    Ok(())
}

async fn on_reaction(event: OriginalSyncReactionEvent, room: Room) {
    // this is going to be good ;)
    dbg!(&event);
    dbg!(event.content.relates_to.key);
}

async fn scan_room_history(room: Room) {
    fn event_content(event: AnySyncTimelineEvent) -> Option<String> {
        match event {
            AnySyncTimelineEvent::MessageLike(event) => match event {
                AnySyncMessageLikeEvent::RoomMessage(SyncMessageLikeEvent::Original(event)) => {
                    Some(event.content.msgtype.body().to_owned())
                }
                AnySyncMessageLikeEvent::Reaction(SyncMessageLikeEvent::Original(event)) => {
                    //dbg!(event);
                    None
                }
                _ => None,
            },
            _ => None,
        }
    }

    let mut opt = MessagesOptions::backward();
    opt.limit = 100.try_into().unwrap();

    //let s = vec!["*".to_string()];
    //opt.filter.types = Some(s.as_ref());
    let backward_stream = room.timeline_backward().await.unwrap();

    pin_mut!(backward_stream);

    while let Some(event) = backward_stream.next().await {
        let event = event.unwrap().event.deserialize().unwrap();
        if let Some(related) = event.relations() {
            dbg!(related);
            /*
            {
                annotation: Some(
                    AnnotationChunk {
                        chunk: [
                            BundledAnnotation {
                                annotation_type: Reaction,
                                key: "✅",
                                origin_server_ts: None,
                                count: 1,
                            },
                        ],
                        next_batch: None,
                    },
                ),
                replace: None,
            }
            */
            // not helpfull, only includes emoji and not username of sender
        }

        use chrono::offset::Utc;
        use chrono::DateTime;
        let datetime: DateTime<Utc> = event.origin_server_ts().to_system_time().unwrap().into();
        if let Some(content) = event_content(event.clone()) {
            let links = extract_urls(content);
            if !links.is_empty() {
                let print: Vec<String> = links
                    .iter()
                    .map(|url| match parse_url(url.clone()) {
                        Some(link) => format!(
                            "{}:{}:{}",
                            link.service,
                            link.kind.map_or_else(|| "".to_string(), |v| v.to_string()),
                            link.id
                        ),
                        None => url.to_string(),
                    })
                    .collect();
                if !print.is_empty() {
                    //println!("{} {} {:#?}", datetime, event.sender(), print);
                }
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct RoomAncestry {
    pub lineage: Vec<Result<Room, OwnedRoomId>>,
    pub offset: usize,
}

impl RoomAncestry {
    async fn get(room: Room) -> RoomAncestry {
        //! scan for descendants and ancestors of room,
        //! TODO joining if needed and possible.

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
}

#[derive(Parser, Debug)]
struct Cli {
    /// The homeserver to connect to.
    #[clap(value_parser)]
    homeserver: Url,

    /// The user name that should be used for the login.
    #[clap(value_parser)]
    username: String,

    /// The password that should be used for the login.
    #[clap(value_parser)]
    password: String,

    #[clap(short, long)]
    device: Option<String>,

    /// Enable verbose logging output.
    #[clap(short, long, action)]
    verbose: bool,
}

async fn login(cli: Cli) -> Result<Client> {
    let home = PathBuf::from(".matrix_crypto_cache");
    //let store = matrix_sdk_sled::make_store_config(path, passphrase)?;

    let builder = Client::builder()
        .homeserver_url(cli.homeserver)
        .handle_refresh_tokens()
        .sled_store(home, None)?;

    let client = builder.build().await?;

    let mut login = client.login_username(&cli.username, &cli.password);

    if let Some(device) = &cli.device {
        // TODO use this https://github.com/matrix-org/matrix-rust-sdk/commit/945c16a7fbe63ecb142a34d2a6bf2682ec67c86f
        // to figure out how to get device from the sled automatically
        login = login.device_id(device);
    }

    let response = login.initial_device_display_name("rust-sdk").send().await?;
    dbg!(response.device_id);

    Ok(client)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt::init();
    }

    let client = login(cli).await?;

    sync(client).await?;

    Ok(())
}
