use crate::identity::IdentityManager;
use crate::{Address, AddressType};
use std::convert::TryInto;
use matrix_sdk::{
    self,
    api::r0::room::create_room::Request,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        SyncMessageEvent,
    },
    Client, ClientConfig, EventEmitter, SyncRoom, SyncSettings,
};
use tokio::time::{self, Duration};
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
    pub async fn start(&'static mut self) {
        self.client
            .add_event_emitter(Box::new(MessageHandler::new(self.manager)));

        // Blocks forever
        join!(
            // Room initializer
            self.room_init(),
            // Message responder
            self.client.sync_forever(SyncSettings::new(), |_| async {})
        );
    }
    pub async fn room_init(&'static self) {
        let mut interval = time::interval(Duration::from_secs(3));

        loop {
            interval.tick().await;

            for ident in self
                .manager
                .get_uninitialized_channel(AddressType::Riot(Address("".to_string())))
            {
                // TODO: Fix this
                let to_invite = [ident.address().0.clone().try_into().unwrap()];

                let mut request = Request::default();
                request.invite = &to_invite;

                self.client.create_room(request).await;
            }
        }
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

#[async_trait]
impl<'a> EventEmitter for MessageHandler<'a> {
    async fn on_room_message(&self, room: SyncRoom, event: &SyncMessageEvent<MessageEventContent>) {
        if let SyncRoom::Joined(room) = room {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::IdentityManager;
    use rocksdb::DB;
    use std::env;
    use std::future::Future;
    use tokio::runtime::Runtime;

    // Convenience function for running async tasks
    fn run<F: Future>(future: F) {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(future);
    }

    // Convenience function
    async fn client<'a>(manager: &'a IdentityManager) -> MatrixClient<'a> {
        MatrixClient::new(
            MatrixConfig {
                homeserver_url: env::var("TEST_MATRIX_HOMESERVER").unwrap(),
                username: env::var("TEST_MATRIX_USER").unwrap(),
                password: env::var("TEST_MATRIX_PASSWORD").unwrap(),
            },
            manager,
        )
        .await
    }

    #[test]
    fn matrix_login_and_sync() {
        let mut manager = IdentityManager::new(DB::open_default("/tmp/test_matrix").unwrap());
        run(client(&manager))
    }
}
