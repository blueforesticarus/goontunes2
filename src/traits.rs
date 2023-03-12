use crate::types::{Message, Reaction};
use crate::utils::channel::{Channel, Mpsc};
//XXX: What is the correct way to define this. I still don't know

pub type MessageChannel = Mpsc<Message, 100>;
pub type ReactChannel = Mpsc<Reaction, 100>;

pub trait ChatService {
    // Note: weirdness with ambiguous type
    // Note cannot return &mut of Receiver because then you couldn't poll simultaneously (can't have 2 &mut self)
    fn message_channel(&mut self) -> <MessageChannel as Channel>::Receiver;
    fn react_channel(&mut self) -> <ReactChannel as Channel>::Receiver;

    //fn rescan_since();
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
}
