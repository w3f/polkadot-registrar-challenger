use url::Url;

use matrix_sdk::{
    self,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        SyncMessageEvent,
    },
    Client, ClientConfig, EventEmitter, SyncRoom, SyncSettings,
};
pub struct MatrixClient {
    config: MatrixConfig,
    client: Client, // `Client` from matrix_sdk
}

pub struct MatrixConfig {
    homeserver_url: String,
    username: String,
    password: String,
}

impl MatrixClient {
    pub async fn new(config: MatrixConfig) -> Self {
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
        }
    }
    pub async fn start(&self) -> Result<(), ()> {
        // Blocks forever
        self.client
            .sync_forever(SyncSettings::new(), |_| async {})
            .await;
        Ok(())
    }
}

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
