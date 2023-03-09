use crate::types::{Message, Reaction};
use crate::utils::channel::{Channel, Mpsc};
//XXX: What is the correct way to define this. I still don't know

pub type MessageChannel = Mpsc<Message, 100>;
pub type ReactChannel = Mpsc<Reaction, 100>;

pub trait ChatService {
    // Note: weirdness with ambiguous type
    fn message_channel(&mut self) -> &mut <MessageChannel as Channel>::Receiver;
    fn react_channel(&mut self) -> &mut <ReactChannel as Channel>::Receiver;

    //fn rescan_since();
}
