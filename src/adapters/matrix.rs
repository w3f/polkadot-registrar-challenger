use crate::identity::IdentityManager;
use crate::AddressType;
use matrix_sdk::{
    self,
    api::r0::room::create_room::Request,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnyMessageEventContent,
    },
    Client, SyncSettings,
};
use std::convert::TryInto;
use tokio::time::{self, Duration};
use url::Url;

pub struct MatrixClient<'a> {
    client: Client, // `Client` from matrix_sdk
    manager: &'a IdentityManager<'a>,
}

pub struct MatrixConfig {
    homeserver_url: String,
    username: String,
    password: String,
}

impl<'a> MatrixClient<'a> {
    pub async fn new(config: MatrixConfig, manager: &'a IdentityManager<'a>) -> MatrixClient<'a> {
        // Setup client
        let homeserver_url =
            Url::parse(&config.homeserver_url).expect("Couldn't parse the homeserver URL");
        let client = Client::new(homeserver_url).unwrap();

        // Login with credentials
        client
            .login(&config.username, &config.password, None, Some("rust-sdk"))
            .await
            .unwrap();

        // Sync up, avoid responding to old messages.
        client.sync(SyncSettings::default()).await.unwrap();

        MatrixClient {
            client: client,
            manager: manager,
        }
    }
    pub async fn start(&mut self) {
        let mut interval = time::interval(Duration::from_secs(1));

        // Blocks forever
        join!(
            // Room initializer
            self.room_init(),
            // Client sync
            async {
                // `sync_forever` results in a panic. Related:
                // https://github.com/rust-lang/rust/issues/64496
                loop {
                    interval.tick().await;
                    self.client.sync(SyncSettings::new()).await.unwrap();
                }
            }
        );
    }
    pub async fn room_init(&self) {
        let mut interval = time::interval(Duration::from_secs(3));

        let db = &self.manager.db.scope("matrix_rooms");

        loop {
            interval.tick().await;

            for ident in self.manager.get_uninitialized_channel(AddressType::Riot) {
                // NOTE/TODO: `unwrap`s will be handled.

                let address = &ident.address().0;

                let room_id = if let Some(val) = db.get(address).unwrap() {
                    std::str::from_utf8(&val).unwrap().try_into().unwrap()
                } else {
                    // TODO: Handle this better
                    let to_invite = [ident.address().0.clone().try_into().unwrap()];

                    let mut request = Request::default();
                    request.invite = &to_invite;
                    request.name = Some("W3F Registrar Verification");

                    let resp = self.client.create_room(request).await.unwrap();
                    db.put(&ident.address().0, resp.room_id.as_str()).unwrap();
                    resp.room_id
                };

                self.client
                    .room_send(
                        &room_id,
                        AnyMessageEventContent::RoomMessage(MessageEventContent::Text(
                            // TODO: Make a proper Message Creator for this
                            TextMessageEventContent::plain(
                                include_str!("../../messages/instructions")
                                    .replace("{:PAYLOAD}", &ident.addr_state.challenge.0),
                            ),
                        )),
                        None,
                    )
                    .await
                    .unwrap();

                // Prevent the sending of multiple messages.
                ident.addr_state.attempt_contact();

                // TODO: Handle response
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::identity::{AddressState, IdentityManager, OnChainIdentity};
    use crate::{Address, AddressType, PubKey};
    use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
    use std::env;
    use std::future::Future;
    use tokio::runtime::Runtime;

    // Convenience function for running async tasks
    fn run<F: Future>(future: F) {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(future);
    }

    // Convenience function
    async fn client<'a>(manager: &'a IdentityManager<'a>) -> MatrixClient<'a> {
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
        let db = Database::new("/tmp/test_matrix").unwrap();
        let manager = IdentityManager::new(&db).unwrap();
        run(client(&manager));
    }

    #[test]
    fn matrix_send_msg() {
        let db = Database::new("/tmp/test_matrix").unwrap();
        let mut manager = IdentityManager::new(&db).unwrap();
        manager
            .register_request(OnChainIdentity {
                pub_key: PubKey(SchnorrkelPubKey::default()),
                display_name: None,
                legal_name: None,
                email: None,
                web: None,
                twitter: None,
                riot: Some(AddressState::new(
                    Address("@fabio:web3.foundation".to_string()),
                    AddressType::Riot,
                )),
            })
            .unwrap();

        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut client = client(&manager).await;
            client.start().await;
        });
    }
}
