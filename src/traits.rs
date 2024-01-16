use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[async_trait]
pub trait ChatService {
    async fn rescan(&self, since: DateTime<Utc>);
}

/*
#[async_trait]
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
    async fn get_track(&self, id: types::Uri) -> eyre::Result<Track>;

    async fn list_playlists(&self, user: Option<types::Uri>) -> eyre::Result<Vec<PlaylistMeta>>;
    //async fn create_playlist(&self, meta: PlaylistMeta) -> eyre::Result<PlaylistId>;

    //async fn get(&self, id: types::Uri) -> eyre::Result<Item>;
    async fn playlist_sync(
        &self,
        current: Option<Collection>,
        playlist: Collection,
    ) -> eyre::Result<()>;
}
*/

/*
/// A trait that defines a *stable* example value, to be used in tests, help messages, and example config generation.
pub trait Example {
    fn example() -> Self;
}

impl Example for DateTime<Utc> {
    fn example() -> Self {
        Utc.with_ymd_and_hms(2014, 7, 8, 9, 10, 11).unwrap() // `2014-07-08T09:10:11Z`
    }
}
*/
