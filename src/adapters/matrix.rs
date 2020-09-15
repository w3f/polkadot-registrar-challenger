use crate::identity::CommsVerifier;
use crate::RoomId;
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

pub struct MatrixClient {
    client: Client, // `Client` from matrix_sdk
    comms: CommsVerifier,
}

pub struct MatrixConfig {
    homeserver_url: String,
    username: String,
    password: String,
}

impl MatrixClient {
    pub async fn new(config: MatrixConfig, comms: CommsVerifier) -> MatrixClient {
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
            comms: comms,
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
        let (context, challenge, room_id) = self.comms.recv_inform();

        let pub_key = context.pub_key;
        let address = context.address;

        // If a room already exists, don't create a new one.
        let room_id = if let Some(room_id) = room_id {
            room_id.0.try_into().unwrap()
        } else {
            // TODO: Handle this better.
            let to_invite = [address.0.clone().try_into().unwrap()];

            let mut request = Request::default();
            request.invite = &to_invite;
            request.name = Some("W3F Registrar Verification");

            let resp = self.client.create_room(request).await.unwrap();

            self.comms
                .track_room_id(&pub_key, &RoomId(resp.room_id.as_str().to_string()));
            resp.room_id
        };

        self.client
            .room_send(
                &room_id,
                AnyMessageEventContent::RoomMessage(MessageEventContent::Text(
                    // TODO: Make a proper Message Creator for this
                    TextMessageEventContent::plain(
                        include_str!("../../messages/instructions")
                            .replace("{:PAYLOAD}", &challenge.0),
                    ),
                )),
                None,
            )
            .await
            .unwrap();

        // TODO: Handle response

        self.comms.valid_feedback(&pub_key, &address);
    }
}
