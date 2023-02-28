// lets use surreal db
// this time lets build from the ground up for persistence

use chrono::{offset::Utc, DateTime};
use strum::{Display, EnumString};

use crate::links::Link;

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString)]
#[strum(ascii_case_insensitive)]
#[strum(serialize_all = "lowercase")]
pub enum MusicService {
    Spotify,
    Youtube,
    Soundcloud,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString)]
#[strum(ascii_case_insensitive)]
#[strum(serialize_all = "lowercase")]
pub enum ChatService {
    Discord,
    Matrix,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub service: ChatService,
    pub date: DateTime<Utc>,
    pub id: String,
    pub sender: String,
    pub links: Vec<Link>,
}

//TODO redo this
#[derive(Debug, Clone)]
pub struct ReactionInner {
    pub id: String,
    pub sender: String,
    pub date: DateTime<Utc>,
}
#[derive(Debug, Clone)]
pub struct Reactions {
    pub service: ChatService,
    pub target: String,
    pub reacts: Vec<ReactionInner>,
}
