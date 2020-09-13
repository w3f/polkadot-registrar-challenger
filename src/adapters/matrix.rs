use crate::identity::IdentityManager;
use matrix_sdk::{
    self,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        SyncMessageEvent,
    },
    Client, ClientConfig, EventEmitter, SyncRoom, SyncSettings,
};
use url::Url;
pub struct MatrixClient<'a> {
    config: MatrixConfig,
    client: Client, // `Client` from matrix_sdk
    manager: &'a IdentityManager,
}

pub struct MatrixConfig {
    homeserver_url: String,
    username: String,
    password: String,
}

impl<'a> MatrixClient<'a> {
    pub async fn new(config: MatrixConfig, manager: &'a IdentityManager) -> MatrixClient<'a> {
        // Setup client
        let homeserver_url =
            Url::parse(&config.homeserver_url).expect("Couldn't parse the homeserver URL");
        let mut client = Client::new(homeserver_url).unwrap();

        // Login with credentials
        client
            .login(&config.username, &config.password, None, Some("rust-sdk"))
            .await
            .unwrap();

        // Sync up, avoid responding to old messages.
        client.sync(SyncSettings::default()).await.unwrap();

        MatrixClient {
            config: config,
            client: client,
            manager: manager,
        }
    }
    pub async fn start(&'static mut self) -> Result<(), ()> {
        self.client
            .add_event_emitter(Box::new(MessageHandler::new(self.manager)));

        // Blocks forever
        self.client
            .sync_forever(SyncSettings::new(), |_| async {})
            .await;
        Ok(())
    }
}

struct MessageHandler<'a> {
    manager: &'a IdentityManager,
}

impl<'a> MessageHandler<'a> {
    fn new(manager: &'a IdentityManager) -> Self {
        MessageHandler { manager: manager }
    }
}

impl<'a> EventEmitter for MessageHandler<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::future::Future;
    use tokio::runtime::Runtime;

    // Convenience function for running async tasks
    fn run<F: Future>(future: F) {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(future);
    }

    // Convenience function
    async fn client() -> MatrixClient {
        MatrixClient::new(MatrixConfig {
            homeserver_url: env::var("TEST_MATRIX_HOMESERVER").unwrap(),
            username: env::var("TEST_MATRIX_USER").unwrap(),
            password: env::var("TEST_MATRIX_PASSWORD").unwrap(),
        })
        .await
    }

    #[test]
    fn matrix_login_and_sync() {
        run(client())
    }
}
