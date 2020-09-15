use crate::identity::CommsVerifier;
use crate::{Result, RoomId};
use matrix_sdk::{
    self,
    api::r0::room::create_room::Request,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnyMessageEventContent, SyncMessageEvent,
    },
    Client, ClientConfig, EventEmitter, JsonStore, SyncRoom, SyncSettings,
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
        comms_emmiter: CommsVerifier,
    ) -> MatrixClient {
        // Setup client
        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let mut client = Client::new(homeserver).unwrap();

        // Login with credentials
        client
            .login(username, password, None, Some("rust-sdk"))
            .await
            .unwrap();

        // Sync up, avoid responding to old messages.
        client.sync(SyncSettings::default()).await.unwrap();

        // Add event emitter (responder)
        client.add_event_emitter(Box::new(Responder::new(client.clone(), comms_emmiter)));

        MatrixClient {
            client: client,
            comms: comms,
        }
    }
    async fn sync_client(&self) -> Result<()> {
        Ok(self.client.sync(SyncSettings::new()).await.map(|_| ())?)
    }
    pub async fn start(self) -> Result<()> {
        println!("Starting room init loop...");

        loop {
            println!("Waiting for messages...");
            // TODO: Improve async situation here..
            let (context, challenge, room_id) = self.comms.recv_inform().await;
            println!("Message received");

            let pub_key = context.pub_key;
            let address = context.address;

            self.sync_client().await.unwrap();

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
                    create_msg(
                        include_str!("../../messages/instructions")
                            .replace("{:PAYLOAD}", &challenge.0)
                            .as_str(),
                    ),
                    None,
                )
                .await
                .unwrap();

            self.sync_client().await.unwrap();
        }

        Ok(())
    }
}

fn create_msg(content: &str) -> AnyMessageEventContent {
    AnyMessageEventContent::RoomMessage(MessageEventContent::Text(
        // TODO: Make a proper Message Creator for this
        TextMessageEventContent::plain(content),
    ))
}

struct Responder {
    client: Client,
    comms: CommsVerifier,
}

impl Responder {
    fn new(client: Client, comms: CommsVerifier) -> Self {
        Responder {
            client: client,
            comms: comms,
        }
    }
}

#[async_trait]
impl EventEmitter for Responder {
    async fn on_room_message(&self, room: SyncRoom, event: &SyncMessageEvent<MessageEventContent>) {
        if let SyncRoom::Joined(room) = room {
            let members = &room.read().await.joined_members;

            if members.len() > 2 {}

            let my_user_id = self.client.user_id().await.unwrap();
            let target_user_id = members
                .iter()
                .map(|(user_id, _)| user_id)
                .find(|user_id| user_id.as_str() != my_user_id.as_str());

            let msg_body = if let SyncMessageEvent {
                content: MessageEventContent::Text(TextMessageEventContent { body: msg_body, .. }),
                ..
            } = event
            {
                msg_body.clone()
            } else {
                return;
            };
        }
    }
}
