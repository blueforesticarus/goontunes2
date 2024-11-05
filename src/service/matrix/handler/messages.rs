use matrix_sdk::{
    event_handler::Ctx,
    ruma::events::{
        reaction::OriginalSyncReactionEvent,
        room::message::{MessageType, OriginalSyncRoomMessageEvent},
    },
    Room,
};

use crate::{prelude::*, service::matrix::Module};

/// Realtime handler for messages
pub async fn on_room_message(
    event: OriginalSyncRoomMessageEvent,
    room: Room,
    client: Ctx<Arc<Module>>,
) {
    // First, we need to unpack the message: We only want messages from rooms we are
    // still in and that are regular text messages - ignoring everything else.

    match &event.content.msgtype {
        MessageType::Text(content) => {
            client
                .process_maybe_command(&event, &room)
                .await
                .log_and_drop::<OnError>();
            client
                .process_message(&event, &room)
                .await
                .log_and_drop::<OnError>();
        }
        other => {
            dbg!(other);
        }
    }
}

/// Realtime handler for emoji reacts
pub async fn on_reaction(event: OriginalSyncReactionEvent, room: Room, client: Ctx<Arc<Module>>) {
    client
        .process_reaction(&event, &room)
        .await
        .log_and_drop::<OnError>();
}
