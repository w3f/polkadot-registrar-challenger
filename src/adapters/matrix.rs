use crate::identity::CommsVerifier;
use crate::{Result, RoomId};
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

impl MatrixClient {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        comms: CommsVerifier,
    ) -> MatrixClient {
        // Setup client
        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let client = Client::new(homeserver).unwrap();

        // Login with credentials
        client
            .login(username, password, None, Some("rust-sdk"))
            .await
            .unwrap();

        // Sync up, avoid responding to old messages.
        client.sync(SyncSettings::default()).await.unwrap();

        MatrixClient {
            client: client,
            comms: comms,
        }
    }
    /// Sync the Matrix client. Syncing was separated from `start` since
    /// running `sync` within `join!` results in a panic. Slightly Related:
    /// https://github.com/rust-lang/rust/issues/64496.
    pub async fn start_sync(&self) {
        // Running `sync_forever` also results in a panic... so just loop.
        let mut interval = time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            self.client.sync(SyncSettings::new()).await.unwrap();
        }
    }
    pub async fn start(&self) -> Result<()> {
        println!("Starting room init loop...");
        loop {
            self.room_init().await;
        }

        Ok(())
    }
    async fn room_init(&self) {
        println!("Waiting for messages...");
        let (context, challenge, room_id) = self.comms.recv_inform();
        println!("Message received");

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

        // self.comms.valid_feedback(&pub_key, &address);
    }
}
