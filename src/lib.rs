use std::fs::File;

enum ServiceConfig {}

enum PlaylistMember {
    //Channel(Channel)
    //Playlist(Playlist)
    //File(File)
}

struct PlaylistConfig {
    inputs: Vec<PlaylistMember>,
    outputs: Vec<PlaylistMember>,
    sync: Vec<PlaylistMember>, // is input and output (ex. file)

    transforms: (), // filter, transform, bash script
}

struct Config {
    services: Vec<ServiceConfig>,
    playlists: Vec<PlaylistConfig>,
}

mod data;
pub mod links;
mod service {
    mod spotify;
    mod youtube;
}
