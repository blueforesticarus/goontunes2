pub use crate::prelude::*;

use crate::service::matrix::{handler, AuthConfig, Module};

use eyre::Context;
use matrix_sdk::{
    config::{StoreConfig, SyncSettings},
    ruma::api::client::filter::FilterDefinition,
};
use std::path::PathBuf;

impl Module {
    #[throws(eyre::Report)]
    #[tracing::instrument(err, skip(self))]
    pub async fn init(self: Arc<Self>) {
        let config = self.config.clone();

        // Create crypto store
        let mut home: PathBuf = shellexpand::full(&config.matrix_crypto_store)?
            .to_string()
            .try_into()?;

        std::fs::create_dir_all(&home)?; //TODO I don't like creating, .cache if it doesn't exist
        home.push(&config.username);

        let pass = config.matrix_crypto_pass.as_deref();
        let state_store = matrix_sdk_sqlite::SqliteStateStore::open(&home, pass).await?;
        let crypto_store = matrix_sdk_sqlite::SqliteCryptoStore::open(&home, pass).await?;

        use matrix_sdk_crypto::store::CryptoStore;
        // Check for existing device id (do manually so we can extract device id)
        let device = crypto_store
            .load_account()
            .await
            .context(format!(
                "matrix store corrupted, delete {:?} and redo verification",
                home
            ))?
            .map(|d| d.device_id().to_string());

        let store_config = StoreConfig::new()
            .crypto_store(crypto_store)
            .state_store(state_store);

        let client = {
            let builder = matrix_sdk::Client::builder()
                .homeserver_url(config.homeserver.clone())
                .handle_refresh_tokens()
                .store_config(store_config);

            builder.build().await?
        };

        // Config Login
        let mut login = match config.auth.clone() {
            AuthConfig::Password(password) => client
                .matrix_auth()
                .login_username(&config.username, password.as_str()),
        };

        if let Some(device) = device.as_ref() {
            login = login.device_id(device);
        }

        let display_name = format!(
            "goontunes on {}",
            hostname::get()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|error| {
                    dbg!(error);
                    "UNKNOWN".to_string()
                })
        );

        // Actually login
        let response = login
            .initial_device_display_name(&display_name)
            .send()
            .await?;
        dbg!(response.device_id);

        self.client.set(client.clone()).unwrap();

        client.add_event_handler_context(self);

        // handlers
        handler::install_verification_handlers(&client);
        handler::install_autojoin_handlers(&client);

        // Enable room members lazy-loading, it will speed up the initial sync a lot
        // with accounts in lots of rooms.
        // See <https://spec.matrix.org/v1.6/client-server-api/#lazy-loading-room-members>.
        let filter = FilterDefinition::with_lazy_loading();
        let settings = SyncSettings::default().filter(filter.into());

        // An initial sync to set up state and so our bot doesn't respond to old
        // messages. If the `StateStore` finds saved state in the location given the
        // initial sync will be skipped in favor of loading state from the store
        let res = client.sync_once(settings.clone()).await?;
        let settings = settings.token(res.next_batch);

        // Method 1
        handler::install_main_handlers(&client);

        dbg!("MATRIX READY");

        tokio::spawn(async move {
            client
                .sync(settings)
                .await
                .expect("this should run forever");
        });
    }
}
