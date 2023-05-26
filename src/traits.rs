use chrono::{DateTime, TimeZone, Utc};

use crate::types::{self, Collection, CollectionId, Message, Reaction, Sender, Track};
use crate::utils::channel::{Channel, Mpsc};
//XXX: What is the correct way to define this. I still don't know

#[derive(Debug, Clone, derive_more::From)]
pub enum ChatEvent {
    Message(Message),
    Reaction(Reaction),
}
pub type ChatChannel = Mpsc<ChatEvent, 100>;

#[async_trait::async_trait]
pub trait ChatService {
    // Note: weirdness with ambiguous type
    // Note cannot return &mut of Receiver because then you couldn't poll simultaneously (can't have 2 &mut self)
    fn channel(&self) -> <ChatChannel as Channel>::Receiver;
    async fn rescan(&self, since: DateTime<Utc>);
    async fn get_user_info(&self, user_id: String) -> eyre::Result<Option<Sender>>;
}

#[async_trait::async_trait]
pub trait PlaylistService {
    /*
    async fn fetch_playlist(id : String) -> Collection;
    async fn fetch_playlist_tracks(id : String) -> Collection;
    async fn get_track_id(id : String) string
    async fn playlist_inserttracks(string, []Pl_Ins) int
    async fn deletetracks(string, []Pl_Rm) int
    async fn playlist_description(string, string) error
    async fn create_playlist(string) string
    async fn list_playlists() []Collection
    */

    //async fn fetch_playlist(id: String) -> Collection;
    //async fn fetch_metadata() -> TrackMetaData;
    async fn get_album(&self, id: types::Uri) -> eyre::Result<Collection<Track>>;
    async fn get_playlist(&self, id: types::Uri) -> eyre::Result<Collection<Track>>;

    //async fn get(&self, id: types::Uri) -> eyre::Result<Item>;
}

/// A trait that defines a *stable* example value, to be used in tests, help messages, and example config generation.
pub trait Example {
    fn example() -> Self;
}

impl Example for DateTime<Utc> {
    fn example() -> Self {
        Utc.with_ymd_and_hms(2014, 7, 8, 9, 10, 11).unwrap() // `2014-07-08T09:10:11Z`
    }
}
