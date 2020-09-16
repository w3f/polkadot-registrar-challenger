use crate::identity::CommsVerifier;
use crate::verifier::Verifier;
use crate::{Account, Result, Signature};
use failure::err_msg;
use matrix_sdk::{
    self,
    api::r0::room::create_room::Request,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnyMessageEventContent, SyncMessageEvent,
    },
    identifiers::RoomId,
    Client, ClientConfig, EventEmitter, JsonStore, SyncRoom, SyncSettings,
};
use schnorrkel::sign::Signature as SchnorrkelSignature;
use std::convert::TryInto;
use tokio::time::{self, Duration};
use url::Url;

#[derive(Clone)]
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
        let store = JsonStore::open("/tmp/matrix_store").unwrap();
        let client_config = ClientConfig::new().state_store(Box::new(store));

        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let mut client = Client::new_with_config(homeserver, client_config).unwrap();

        // Login with credentials
        client
            .login(username, password, None, Some("rust-sdk"))
            .await
            .unwrap();

        // Sync up, avoid responding to old messages.
        client.sync(SyncSettings::default()).await.unwrap();

        // Add event emitter (responder)
        client
            .add_event_emitter(Box::new(
                Responder::new(client.clone(), comms_emmiter).await,
            ))
            .await;

        let sync_client = client.clone();
        tokio::spawn(async move {
            sync_client
                .sync_forever(SyncSettings::default(), |_| async {})
                .await;
        });

        MatrixClient {
            client: client,
            comms: comms,
        }
    }
    async fn send_msg(&self, msg: &str, room_id: &RoomId) -> Result<()> {
        send_msg(&self.client, msg, room_id).await
    }
    pub async fn start(self) {
        loop {
            let _ = self.local().await.map_err(|err| {
                // TODO: log...
                err
            });
        }
    }
    async fn local(&self) -> Result<()> {
        let (context, challenge, room_id) = self.comms.recv_inform().await;

        let pub_key = context.pub_key;
        let address = context.address;

        // If a room already exists, don't create a new one.
        let room_id = if let Some(room_id) = room_id {
            room_id
        } else {
            // TODO: Handle this better.
            let to_invite = [address.0.clone().try_into()?];

            let mut request = Request::default();
            request.invite = &to_invite;
            request.name = Some("W3F Registrar Verification");

            let resp = self.client.create_room(request).await?;

            self.comms.track_room_id(&pub_key, &resp.room_id);
            resp.room_id
        };

        self.send_msg(
            include_str!("../../messages/instructions")
                .replace("{:PAYLOAD}", &challenge.0)
                .as_str(),
            &room_id,
        )
        .await?;

        Ok(())
    }
}

async fn send_msg(
    client: &Client,
    msg: &str,
    room_id: &matrix_sdk::identifiers::RoomId,
) -> Result<()> {
    client
        .room_send(
            room_id,
            AnyMessageEventContent::RoomMessage(MessageEventContent::Text(
                // TODO: Make a proper Message Creator for this
                TextMessageEventContent::plain(msg),
            )),
            None,
        )
        .await?;

    Ok(())
}

struct Responder {
    client: Client,
    comms: CommsVerifier,
}

impl Responder {
    async fn new(client: Client, comms: CommsVerifier) -> Self {
        Responder {
            client: client,
            comms: comms,
        }
    }
    async fn send_msg(&self, msg: &str, room_id: &matrix_sdk::identifiers::RoomId) -> Result<()> {
        send_msg(&self.client, msg, room_id).await
    }
    async fn local(
        &self,
        room: SyncRoom,
        event: &SyncMessageEvent<MessageEventContent>,
    ) -> Result<()> {
        // Do not respond to its own messages. It's weird that the EventEmitter
        // even processes its own messages anyway...
        if event.sender
            == self
                .client
                .user_id()
                .await
                .ok_or(err_msg("no user id found"))?
        {
            return Ok(());
        }

        if let SyncRoom::Joined(room) = room {
            let members = &room.read().await.joined_members;

            self.comms
                .request_account_state(&Account(event.sender.as_str().to_string()));
            let (context, challenge, _) = self.comms.recv_inform().await;

            let verifier = Verifier::new(context, challenge);

            // TODO: Write a nicer function for this.
            let msg_body = if let SyncMessageEvent {
                content: MessageEventContent::Text(TextMessageEventContent { body: msg_body, .. }),
                ..
            } = event
            {
                msg_body.clone()
            } else {
                return Ok(());
            };

            let room_id = room.read().await.room_id.clone();

            match verifier.verify(&msg_body) {
                Ok(msg) => self.send_msg(&msg, &room_id).await?,
                Err(err) => self.send_msg(&err.to_string(), &room_id).await?,
            };
        }

        Ok(())
    }
}

#[async_trait]
impl EventEmitter for Responder {
    async fn on_room_message(&self, room: SyncRoom, event: &SyncMessageEvent<MessageEventContent>) {
        let _ = self.local(room, event).await.map_err(|err| {
            // TODO: log...
            err
        });
    }
}
