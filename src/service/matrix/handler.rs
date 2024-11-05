mod autojoin;
mod commands;
mod verification;

pub use autojoin::install_autojoin_handlers;
pub use verification::install_verification_handlers;

mod messages;

use matrix_sdk::Client;
pub fn install_main_handlers(client: &Client) {
    client.add_event_handler(messages::on_room_message);
    client.add_event_handler(messages::on_reaction);
}
