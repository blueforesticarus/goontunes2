use crate::prelude::*;
use eyre::ContextCompat;
use matrix_sdk::{
    ruma::{
        events::room::create::RoomCreateEventContent,
        OwnedRoomId,
    },
    Room,
};

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
    pub async fn get_try_join(room: &Room) -> RoomAncestry {
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
                        .log_and_drop::<Bug>();

                    // loop untill there are no unjoined rooms which we haven't tried to join.
                    // joining a room may cause new room to show up in ancestry
                    continue;
                }
            }

            return ancestry;
        }
    }

    pub async fn get(room: &Room) -> RoomAncestry {
        //! scan for descendants and ancestors of room,
        let mut ret = RoomAncestry::default();
        let client = room.client().clone();

        async fn get_previous(room: Room) -> Result<Option<OwnedRoomId>> {
            let created = room
                .get_state_events_static::<RoomCreateEventContent>()
                .await?;

            let Some(created) = created.get(0) else {
                return Ok(None);
            };
            let event = created.deserialize()?;
            let event = event.as_sync().context("room is in invited state")?;
            let event = event.as_original().context("room creation redacted?")?;
            Ok(event
                .content
                .predecessor
                .clone()
                .map(|previous_room| previous_room.room_id))
        }

        let mut current = room.clone();
        while let Some(roomid) = get_previous(current)
            .await
            .log::<Bug>()
            .unwrap_or_default()
        {
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
