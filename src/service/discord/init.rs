use serenity::all::{ChannelId, GetMessages, Http};

use crate::prelude::*;

use crate::service::discord::db::add_message;
use crate::service::discord::{
    db::{add_channel, add_guild, get_last_message_for_channel},
    Module,
};
use crate::utils::when_even::Ignoreable;

impl Module {
    #[throws(eyre::Report)]
    #[tracing::instrument(skip(self))]
    pub async fn init(self: Arc<Self>) {
        self.listen().await;

        for guild in self.http().get_guilds(None, None).await? {
            add_guild(&self.db, guild.clone())
                .await
                .log_and_drop::<OnError>();

            for channel in self
                .http()
                .get_channels(guild.id)
                .await
                .log::<OnError>()
                .unwrap_or_default()
            {
                add_channel(&self.db, serenity::all::Channel::Guild(channel))
                    .await
                    .log_and_drop::<OnError>();
            }
        }

        for channel in self.config.channels.iter() {
            self.scan_since(ChannelId::new(channel.parse().unwrap()))
                .await
                .log_and_drop::<OnError>()
        }
    }

    #[throws(eyre::Report)]
    #[tracing::instrument(skip(self))]
    async fn scan_since(&self, channel_id: ChannelId) {
        let last = get_last_message_for_channel(&self.db, channel_id)
            .await
            .log::<OnError>()
            .unwrap_or_default();
        let last = last.map(|v| v.ts).unwrap_or_default();

        dbg!(&channel_id, last);

        let mut builder = GetMessages::new();
        loop {
            let msgs = channel_id.messages(self.http(), builder).await?;

            // TODO: This is wastefully slow for external db, but also I should batch the updates anyway
            for msg in msgs.iter() {
                add_message(&self.db, msg.clone())
                    .await
                    .log_and_drop::<OnError>();
            }

            let Some(oldest) = msgs.iter().min_by_key(|v| v.timestamp.to_utc()) else {
                break;
            };

            dbg!(oldest.timestamp.to_utc());

            if oldest.timestamp.to_utc() < last {
                break;
            }

            //XXX issue with interruption
            builder = builder.before(oldest.id);
        }

        dbg!();
    }

    fn http(&self) -> &Arc<Http> {
        self.http.get().unwrap()
    }
}

impl AsRef<Http> for Module {
    fn as_ref(&self) -> &Http {
        self.http.get().unwrap()
    }
}
