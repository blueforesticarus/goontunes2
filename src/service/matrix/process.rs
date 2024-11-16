use std::time::Duration;

use crate::{prelude::*, service::matrix::util::RoomAncestry};

use enum_extract::let_extract;
use eyre::{bail, ContextCompat};
use matrix_sdk::{
    room::MessagesOptions,
    ruma::{
        events::{
            reaction::OriginalSyncReactionEvent,
            room::{
                create::{OriginalSyncRoomCreateEvent, RoomCreateEventContent},
                message::OriginalSyncRoomMessageEvent,
            },
            AnyMessageLikeEvent, AnySyncMessageLikeEvent, AnySyncTimelineEvent, AnyTimelineEvent,
            MessageLikeEvent, SyncMessageLikeEvent,
        },
        OwnedRoomId, OwnedUserId, RoomId,
    },
    Room, RoomState,
};
use tracing::warn;

use super::Module;

impl Module {
    /// Process a (possibly new, possibly old) emoji react
    #[throws(eyre::Report)]
    pub async fn process_reaction(&self, event: &OriginalSyncReactionEvent, room: &Room) {
        // TODO reprocess target to see if it has a url, so we don't hammer db for no reason
        // however, implement db to handle reactions to messages it doesn't know about so we can skip them
        // (also implement db to handle messages with no link)
        let event_id = event.content.relates_to.event_id.to_owned();

        // let data = Reaction {
        //     sender: types::SenderId {
        //         service: ChatService::Matrix,
        //         id: event.sender.to_string(),
        //     },
        //     target: types::MessageId(event_id.to_string()),
        //     date: event
        //         .origin_server_ts
        //         .to_system_time()
        //         .ok_or_else(|| eyre::eyre!("weird date {:?}", event.origin_server_ts))?
        //         .into(),
        //     id: types::ReactionId(event_id.to_string()),
        //     txt: vec![event.content.relates_to.key],
        // };

        // NOTE: messages are not state events
        let message = room.event(&event_id).await?;
        let message: AnySyncTimelineEvent = message.event.deserialize()?.into();
        let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(message)) =
            message
        else {
            bail!("react target not a room message {:?}", message);
        };

        match message {
            SyncMessageLikeEvent::Redacted(_) => {}
            SyncMessageLikeEvent::Original(message) => {
                // TODO
                dbg!(message);
            }
        };
    }

    /// Process a (possibly new, possibly old) message
    #[throws(eyre::Report)]
    pub async fn process_message(&self, event: &OriginalSyncRoomMessageEvent, room: &Room) {
        dbg!(event.content.body());

        // TODO move this
        let link = crate::utils::links::extract_links(event.content.body());

        if event.content.body().is_empty() {
            return;
        }

        use surrealdb::sql::Thing;
        #[derive(Debug, Serialize, Deserialize)]
        struct MessageBundle {
            id: Thing,
            service: crate::types::chat::Service,
            message: crate::types::chat::Message,
            user: Thing,
            channel: Thing,
            link: Vec<crate::types::Link>,
        }

        let message = MessageBundle {
            id: Thing::from(("message".to_string(), event.event_id.to_string())),
            service: types::chat::Service::Matrix,
            message: crate::types::chat::Message {
                text: event.content.body().to_string(),
                timestamp: DateTime::from(event.origin_server_ts.to_system_time().unwrap()),
            },
            user: Thing::from(("user".to_string(), event.sender.to_string())),
            channel: Thing::from(("channel".to_string(), room.room_id().to_string())),
            link,
        };

        let _: Vec<MessageBundle> = self
            .db
            .insert("message")
            .content(message)
            .await
            .log::<Bug>()
            .unwrap_or_default();

        // let data = types::Message {
        //     id: types::MessageId(event.event_id.to_string()),
        //     channel: types::ChannelId {
        //         service: ChatService::Matrix,
        //         id: room.room_id().to_string(),
        //     },
        //     sender: types::SenderId {
        //         service: ChatService::Matrix,
        //         id: event.sender.to_string(),
        //     },
        //     date: event
        //         .origin_server_ts
        //         .to_system_time()
        //         .ok_or_else(|| eyre::eyre!("weird date {:?}", event.origin_server_ts))?
        //         .into(),
        //     links,
        // };
    }

    pub async fn scan_room_and_ancestors(&self, target: &Room, since: DateTime<Utc>) {
        let ancestry = RoomAncestry::get_try_join(target).await;
        println!("{}", ancestry);
        for target in ancestry.lineage.into_iter().filter_map(Result::ok).rev() {
            self.scan_room_history(&target, since).await;

            //TODO clean this up, really no need for all this code just to find the create event (again) and it's timestamp (again)
            #[throws(eyre::Report)]
            async fn get_create_event(target: Room) -> OriginalSyncRoomCreateEvent {
                let events = target
                    .get_state_event_static::<RoomCreateEventContent>()
                    .await?;

                events
                    .context("no createRoom event")?
                    .deserialize()?
                    .as_sync()
                    .context("stripped state")?
                    .as_original()
                    .context("room reaction redacted???")?
                    .clone()
            }

            let event = match get_create_event(target).await.log::<Bug>() {
                Ok(v) => v,
                Err(_) => continue,
            };

            let dt: DateTime<Utc> = event.origin_server_ts.to_system_time().unwrap().into();
            if dt > since {
                // this room was created after the since date, so previous room was ended before earliest messages we are scanning for
                break;
            }
        }
    }

    #[throws(eyre::Report)]
    pub async fn scan_room_history(&self, target: &Room, since: DateTime<Utc>) {
        println!("scanning {}", target.room_id());
        let mut end: Option<String> = None;

        loop {
            let mut opt = MessagesOptions::backward();
            opt.limit = 100.try_into().unwrap();
            opt.from = end.clone();

            let page = target.messages(opt).await?;

            let mut ts = Utc::now();
            for event in page.chunk {
                let event = event.event.deserialize()?;

                ts = ts.min(event.origin_server_ts().to_system_time().unwrap().into());

                let_extract!(AnyTimelineEvent::MessageLike(event), event, continue);

                //TODO some kind of log
                match event {
                    AnyMessageLikeEvent::RoomMessage(MessageLikeEvent::Original(event)) => self
                        .process_message(&event.into(), target)
                        .await
                        .log_and_drop::<Bug>(),
                    AnyMessageLikeEvent::Reaction(MessageLikeEvent::Original(event)) => self
                        .process_reaction(&event.into(), target)
                        .await
                        .log_and_drop::<Bug>(),
                    AnyMessageLikeEvent::RoomEncrypted(e) => {
                        warn!("encrypted");
                    }
                    _ => {}
                }
            }

            if ts < since {
                break;
            }

            end = page.end;
            if end.is_none() {
                break;
            }
        }
    }

    #[throws(eyre::Report)]
    async fn get_room_try_join(&self, room_id: &RoomId) -> Room {
        let room = self
            .client()
            .get_room(room_id)
            .context(format!("no room {room_id}"))?;

        if room.state() == RoomState::Joined {
            return room;
        }

        self.client()
            .join_room_by_id(room_id)
            .await
            .log::<Bug>();
        tokio::time::sleep(Duration::from_millis(100)).await; // I have no reason to assume this is needed, but I don't feel like debugging

        // XXX idk if this works
        assert_eq!(room.state(), RoomState::Joined);
        room
    }

    fn joined_room_ids(&self) -> Vec<OwnedRoomId> {
        self.client()
            .joined_rooms()
            .into_iter()
            .map(|v| v.room_id().into())
            .collect()
    }

    pub async fn user_info(&self, user_id: OwnedUserId) -> Option<UserData> {
        let id = user_id.to_string();
        let mut alias: Vec<String> = Vec::new();
        let mut avatar: Option<String> = None;

        for room in self.client().joined_rooms() {
            if let Ok(Some(m)) = room.get_member(&user_id).await.log::<Bug>() {
                if let Some(n) = m.display_name() {
                    alias.push(n.to_string())
                }

                alias.push(m.name().to_string());
                avatar = m.avatar_url().map(|v| v.into());
                // if avatar.is_none() {
                //     avatar = m
                //         .avatar(matrix_sdk::media::MediaFormat::File)
                //         .await
                //         .log::<OnError>()
                //         .unwrap_or(None)
                // }
            }
        }

        alias.sort();
        alias.dedup();

        if alias.is_empty() {
            return None;
        }

        Some(UserData { id, alias, avatar })
    }

    // async fn joined(&self, room_id: &RoomId) -> Result<Joined> {
    //     self.client
    //         .get_joined_room(room_id)
    //         .wrap_err_with(|| format!("no joined room {}", room_id))
    // }

    // async fn read_receipt(&self, room_id: &RoomId, event_id: &EventId) -> Result<()> {
    //     self.joined(room_id).await?.read_receipt(event_id).await?;
    //     Ok(())
    // }

    // async fn typing(&self, room_id: &RoomId, on: bool) -> Result<()> {
    //     self.joined(room_id).await?.typing_notice(on).await?;
    //     Ok(())
    // }

    // async fn react_to(&self, room_id: &RoomId, event_id: &EventId, reaction: &str) -> Result<()> {
    //     let r = ReactionEventContent::new(Relation::new(event_id.into(), reaction.to_string()));
    //     let r = AnyMessageLikeEventContent::Reaction(r);

    //     self.joined(room_id).await?.send(r, None).await?;
    //     Ok(())
    // }
}

#[derive(Debug, Clone)]
pub struct UserData {
    id: String,
    alias: Vec<String>,
    avatar: Option<String>,
}
// impl MatrixClient {
//     async fn rescan(&self, since: DateTime<Utc>) {
//         let channels = self
//             .config
//             .channels
//             .clone()
//             .unwrap_or_else(|| self.joined_room_ids());

//         for channel in channels.iter() {
//             if let Ok(joined) = self.get_room_try_join(channel).await.log::<OnError>() {
//                 self.scan_room_and_ancestors(joined.into(), since).await;
//             }
//         }
//     }

//     async fn get_user_info(&self, user_id: String) -> Result<Option<Sender>> {
//         let user_id = OwnedUserId::try_from(user_id)?;
//         Ok(self.user_info(user_id).await)
//     }
// }
