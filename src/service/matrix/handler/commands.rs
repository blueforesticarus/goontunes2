use crate::prelude::*;

use chrono::Duration;
use clap::Parser;
use eyre::ContextCompat;
use matrix_sdk::{
    ruma::{
        events::room::message::{OriginalSyncRoomMessageEvent, RoomMessageEventContent},
        OwnedRoomId, OwnedUserId,
    },
    Room,
};
use tracing::info;

use crate::service::matrix::{util::RoomAncestry, Module};

#[derive(Debug, Clone, clap::Parser)]
pub enum MatrixCommands {
    Ping,
    Scan {
        #[arg(short, long)]
        all: bool,

        channel: Option<OwnedRoomId>,
    },
    Leave {
        channel: Option<OwnedRoomId>,
    },
    Join {
        channel: Option<OwnedRoomId>,
    },
    History {
        channel: Option<OwnedRoomId>,
    },
    User {
        user_id: Option<OwnedUserId>,
    },
}

impl Module {
    pub async fn process_maybe_command(
        &self,
        event: &OriginalSyncRoomMessageEvent,
        room: &Room,
    ) -> Result<bool> {
        let content = event.content.body().trim();

        let username = room.client().account().get_display_name().await?;

        let Some(cmd) = username
            .and_then(|u| content.strip_prefix(format!("{}: ", u).as_str()))
            .or_else(|| content.strip_prefix('!'))
        else {
            return Ok(false);
        };

        // Use command handler
        let mut cmd_txt: Vec<String> = cmd
            .trim_start_matches('!') // permit @bot !command
            .trim()
            .split_ascii_whitespace()
            .map(ToString::to_string)
            .collect();

        // because clap expects exe name in arg0
        cmd_txt.insert(0, "!".into());

        info!("{:?}", cmd_txt);
        let command = MatrixCommands::try_parse_from(cmd_txt.clone());

        //XXX hack to not process old commands
        if Utc::now() - DateTime::from(event.origin_server_ts.to_system_time().unwrap())
            > Duration::seconds(5)
        {
            return Ok(true);
        }

        match command {
            Ok(command) => match self.do_command(command, event, room).await {
                Ok(_) => {}
                Err(e) => {
                    let msg = RoomMessageEventContent::text_plain(e.to_string());
                    room.send(msg).await.log_and_drop::<Bug>();
                }
            },
            Err(e) => {
                let msg = RoomMessageEventContent::text_plain(e.render().to_string());
                room.send(msg).await.log_and_drop::<Bug>();
            }
        }

        Ok(true)
    }

    /// delegate for !commands
    #[throws(eyre::Report)]
    async fn do_command(
        &self,
        command: MatrixCommands,
        event: &OriginalSyncRoomMessageEvent,
        room: &Room,
    ) {
        //TODO, abstract beyond just matrix commands, allow cli as well
        let get_room = |channel: Option<OwnedRoomId>| match &channel {
            Some(room_id) => self
                .client()
                .get_room(room_id)
                .context(format!("no room {channel:?}")),
            None => Ok(room.clone()),
        };

        match command {
            MatrixCommands::Ping => {
                let content = RoomMessageEventContent::text_plain("pong");
                //Note: the last parameter is an optional transaction id
                room.send(content).await?; //XXX
            }
            MatrixCommands::Scan { all, channel } => {
                let target: Room = get_room(channel)?;
                //TODO handle non-existant room
                if all {
                    self.scan_room_and_ancestors(&target, DateTime::<Utc>::MIN_UTC)
                        .await;
                } else {
                    self.scan_room_history(&target, DateTime::<Utc>::MIN_UTC)
                        .await?;
                }
            }
            MatrixCommands::User { user_id } => {
                let info = self
                    .user_info(user_id.unwrap_or(event.sender.clone()))
                    .await;
                let content = RoomMessageEventContent::text_plain(format!("{:#?}", info));
                room.send(content).await?;
            }
            MatrixCommands::History { channel } => {
                let target: Room = get_room(channel)?;
                let history = RoomAncestry::get(&target).await;
                let content = RoomMessageEventContent::text_markdown(history.to_string());
                room.send(content).await?;
            }
            MatrixCommands::Leave { channel } => {
                let target: Room = get_room(channel)?;
                target.leave().await.log_and_drop::<Bug>()
            }
            MatrixCommands::Join { channel } => {
                let target: Room = get_room(channel)?;
                target.join().await.log_and_drop::<Bug>();
            }
        }
    }
}
