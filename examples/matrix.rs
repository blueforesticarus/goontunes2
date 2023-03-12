///
///  This is an example showcasing how to build a very simple bot using the
/// matrix-sdk. To try it, you need a rust build setup, then you can run:
/// `cargo run -p example-getting-started -- <homeserver_url> <user> <password>`
///
/// Use a second client to open a DM to your bot or invite them into some room.
/// You should see it automatically join. Then post `!party` to see the client
/// in action.
///
/// Below the code has a lot of inline documentation to help you understand the
/// various parts and what they do
// The imports we need
use std::{
    env,
    path::PathBuf,
    process::exit,
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use eyre::ContextCompat;
use matrix_sdk::{
    config::SyncSettings,
    room::{MessagesOptions, Room},
    ruma::{
        api::client::{
            device::get_device::v3::Response, filter::RoomEventFilter,
            message::get_message_events::v3::Direction,
        },
        events::{
            room::{
                member::StrippedRoomMemberEvent,
                message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
            },
            AnyTimelineEvent, StateEventType,
        },
        OwnedRoomId, RoomId,
    },
    Client,
};

use matrix_sdk::{ruma::OwnedUserId, LoopCtrl};
use tokio::time::{sleep, Duration};
use url::Url;

async fn bootstrap(client: Client, user_id: OwnedUserId, password: String) {
    println!("Bootstrapping a new cross signing identity, press enter to continue.");

    let mut input = String::new();

    std::io::stdin()
        .read_line(&mut input)
        .expect("error: unable to read user input");

    let ee = client.encryption();
    match ee.cross_signing_status().await {
        Some(s) => {
            dbg!(s);
            let devices = ee
                .get_user_devices(client.user_id().unwrap())
                .await
                .unwrap();
            dbg!(devices);
        }
        None => {
            panic!(); //idk what this means
        }
    };

    // This resets cross signing identity
    /*
    if let Err(e) = client.encryption().bootstrap_cross_signing(None).await {
        use matrix_sdk::ruma::api::client::uiaa;

        if let Some(response) = e.uiaa_response() {
            let mut password = uiaa::Password::new(
                uiaa::UserIdentifier::UserIdOrLocalpart(user_id.as_str()),
                &password,
            );
            password.session = response.session.as_deref();

            client
                .encryption()
                .bootstrap_cross_signing(Some(uiaa::AuthData::Password(password)))
                .await
                .expect("Couldn't bootstrap cross signing")
        } else {
            panic!("Error during cross-signing bootstrap {e:#?}");
        }
    }
    */
}

//https://github.com/tilosp/matrix-send-rs/blob/main/src/matrix.rs
async fn login(homeserver_url: String, username: &str, password: &str) -> eyre::Result<()> {
    /*
    let home = dirs::data_dir()
        .expect("no home directory found")
        .join("getting_started");
    */
    let home = PathBuf::from(".matrix_crypto_cache");
    let homeserver_url = Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
    let client = Client::builder()
        // We use the convenient client builder to set our custom homeserver URL on it.
        .homeserver_url(homeserver_url)
        // Matrix-SDK has support for pluggable, configurable state and crypto-store
        // support we use the default sled-store (enabled by default on native
        // architectures), to configure a local cache and store for our crypto keys
        .handle_refresh_tokens()
        .sled_store(home, None)?
        .build()
        .await?;

    return Ok(());
    let login = client
        .login_username(username, password)
        .initial_device_display_name("rust-sdk");

    let response = login.send().await?;
    dbg!(response.device_id);

    let user_id = &response.user_id;
    let client_ref = &client;
    let asked = AtomicBool::new(false);
    let asked_ref = &asked;

    // Now, we want our client to react to invites. Invites sent us stripped member
    // state events so we want to react to them. We add the event handler before
    // the sync, so this happens also for older messages. All rooms we've
    // already entered won't have stripped states anymore and thus won't fire
    client.add_event_handler(on_stripped_state_member);

    // An initial sync to set up state and so our bot doesn't respond to old
    // messages. If the `StateStore` finds saved state in the location given the
    // initial sync will be skipped in favor of loading state from the store
    let sync_token = client
        .sync_once(SyncSettings::default())
        .await
        .unwrap()
        .next_batch;

    // now that we've synced, let's attach a handler for incoming room messages, so
    // we can react on it
    client.add_event_handler(on_room_message);

    tokio::spawn(bootstrap(
        client.clone(),
        (*user_id).clone(),
        password.to_owned(),
    ));

    // since we called `sync_once` before we entered our sync loop we must pass
    // that sync token to `sync`
    let settings = SyncSettings::default().token(sync_token);
    // this keeps state from the server streaming in to the bot via the
    // EventHandler trait
    client
        .sync_with_callback(settings, |_| async move {
            let asked = asked_ref;
            let client = &client_ref;
            let user_id = &user_id;

            // Wait for sync to be done then ask the user to bootstrap.
            if !asked.load(Ordering::SeqCst) {}

            asked.store(true, Ordering::SeqCst);
            LoopCtrl::Continue
        })
        .await?;

    Ok(())
}

/// This is the starting point of the app. `main` is called by rust binaries to
/// run the program in this case, we use tokio (a reactor) to allow us to use
/// an `async` function run.
#[tokio::main]
async fn main() -> eyre::Result<()> {
    // set up some simple stderr logging. You can configure it by changing the env
    // var `RUST_LOG`
    tracing_subscriber::fmt::init();

    // parse the command line for homeserver, username and password
    let (homeserver_url, username, password) =
        match (env::args().nth(1), env::args().nth(2), env::args().nth(3)) {
            (Some(a), Some(b), Some(c)) => (a, b, c),
            _ => {
                eprintln!(
                    "Usage: {} <homeserver_url> <username> <password>",
                    env::args().next().unwrap()
                );
                // exist if missing
                exit(1)
            }
        };

    // our actual runner
    login(homeserver_url, &username, &password).await?;
    Ok(())
}

// Whenever we see a new stripped room member event, we've asked our client to
// call this function. So what exactly are we doing then?
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

// This fn is called whenever we see a new room message event. You notice that
// the difference between this and the other function that we've given to the
// handler lies only in their input parameters. However, that is enough for the
// rust-sdk to figure out which one to call one and only do so, when
// the parameters are available.
async fn on_room_message(event: OriginalSyncRoomMessageEvent, room: Room) {
    // First, we need to unpack the message: We only want messages from rooms we are
    // still in and that are regular text messages - ignoring everything else.
    dbg!(&event.content.msgtype);
    let Room::Joined(room) = room else { return };
    match event.content.msgtype {
        MessageType::Text(text_content) => {
            // here comes the actual "logic": when the bot see's a `!party` in the message,
            // it responds
            if text_content.body.contains("!party") {
                let content = RoomMessageEventContent::text_plain("🎉🎊🥳 let's PARTY!! 🥳🎊🎉");

                println!("sending");

                // send our message to the room we found the "!party" command in
                // the last parameter is an optional transaction id which we don't
                // care about.
                room.send(content, None).await.unwrap();

                println!("message sent");
            } else if text_content.body.contains("!scan") {
                let room = room
                    .client()
                    //.get_room(&OwnedRoomId::from_str("!hGCtIIRpCBMylILOoc:matrix.org").unwrap())
                    .get_room(&OwnedRoomId::from_str("!mXEFBvHoJyDuNhDkyc:matrix.org").unwrap())
                    .unwrap();

                /*
                room.messages(MessagesOptions::new(Direction::Backward))
                    .await
                    .unwrap()
                    .chunk
                    .into_iter()
                    .for_each(|timeline_event| {
                        let e: AnyTimelineEvent = timeline_event.event.deserialize().unwrap();
                        match e {
                            matrix_sdk::ruma::events::AnyTimelineEvent::MessageLike(s) => match s {
                                /*
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::CallAnswer(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::CallInvite(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::CallHangup(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::CallCandidates(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::KeyVerificationReady(_) => {
                                    todo!()
                                }
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::KeyVerificationStart(_) => {
                                    todo!()
                                }
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::KeyVerificationCancel(_) => {
                                    todo!()
                                }
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::KeyVerificationAccept(_) => {
                                    todo!()
                                }
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::KeyVerificationKey(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::KeyVerificationMac(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::KeyVerificationDone(_) => {
                                    todo!()
                                }
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::Reaction(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomEncrypted(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomMessage(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomRedaction(_) => todo!(),
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::Sticker(_) => todo!(),
                                */
                                matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomMessage(msg) => {
                                    let msg = msg.as_original().unwrap(); // filters redacted
                                    dbg!(msg.content.body());
                                }
                                msg => {
                                    dbg!(msg.event_type());
                                }
                            },
                            matrix_sdk::ruma::events::AnyTimelineEvent::State(_) => {
                                panic!(); // should not happen
                            }
                        };
                    });
                //dbg!(messages);
                */

                let mut opt = MessagesOptions::backward();
                opt.limit = 100.try_into().unwrap();

                //let s = vec!["*".to_string()];
                //opt.filter.types = Some(s.as_ref());
                let messages = room.messages(opt).await.unwrap();

                for c in messages.chunk {
                    let a = c.event.deserialize().unwrap();
                    match a {
                        matrix_sdk::ruma::events::AnyTimelineEvent::MessageLike(v) => match v {
                            matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomEncrypted(_) => {
                                //println!("missing")
                            }
                            matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomMessage(v) => {
                                match v {
                                    matrix_sdk::ruma::events::MessageLikeEvent::Original(v) => {
                                        println!("{} {}", v.sender, v.content.body());
                                    }
                                    matrix_sdk::ruma::events::MessageLikeEvent::Redacted(v) => {
                                        dbg!(v.content);
                                    }
                                }
                            }
                            _ => {}
                        },
                        matrix_sdk::ruma::events::AnyTimelineEvent::State(v) => {
                            dbg!(v);
                        }
                    }
                }
            }
        }
        MessageType::VerificationRequest(v) => {
            dbg!(v.methods);
        }
        other => {
            dbg!(other);
        }
    }
}

// 2022-11-29T06:46:50.828282Z  WARN matrix_sdk_crypto::identities::device: Trying to encrypt a Megolm session for user @cereal_killer:matrix.org on device DDFDNWUPSV, but no Olm session is found
