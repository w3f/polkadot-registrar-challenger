use crate::Result;
use matrix_sdk::{
    self,
    api::r0::room::create_room::{Request, Response},
    api::r0::room::Visibility,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnyMessageEventContent, SyncMessageEvent,
    },
    identifiers::{RoomId, UserId},
    Client, ClientConfig, EventEmitter, JsonStore, SyncRoom, SyncSettings,
};
use std::convert::TryInto;
use std::result::Result as StdResult;
use tokio::time::{self, Duration};
use url::Url;

#[derive(Clone)]
pub struct MatrixClient {
    client: Client, // `Client` from matrix_sdk
}

impl MatrixClient {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        db_path: &str,
    ) -> Result<MatrixClient> {
        info!("Setting up Matrix client");
        // Setup client
        let store = JsonStore::open(db_path)?;
        let client_config = ClientConfig::new().state_store(Box::new(store));

        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let client = Client::new_with_config(homeserver, client_config)?;

        // Login with credentials
        client
            .login(username, password, None, Some("w3f-registrar-bot"))
            .await?;

        // Sync up, avoid responding to old messages.
        info!("Syncing Matrix client");
        client
            .sync(SyncSettings::default())
            .await?;

        // Request a list of open/pending room ids. Used to detect dead rooms.
        //let pending_room_ids = db.select_room_ids().await?;

        // Leave dead rooms.
        /*
        info!("Detecting dead Matrix rooms");
        let rooms = client.joined_rooms();
        let rooms = rooms.read().await;
        for (room_id, _) in rooms.iter() {
            if pending_room_ids.iter().find(|&id| id == room_id).is_none() {
                // TODO: Leave room after 1 day.
                //warn!("Leaving dead room: {}", room_id.as_str());
                //let _ = client.leave_room(room_id).await;
            }
        }
        */

        let sync_client = client.clone();
        tokio::spawn(async move {
            sync_client
                .sync_forever(SyncSettings::default(), |_| async {})
                .await;
        });

        let matrix = MatrixClient { client: client };

        Ok(matrix)
    }
}

impl MatrixClient {
    /*
    async fn send_message(&self, room_id: &RoomId, message: VerifierMessage) -> Result<()> {
        self.client
            .room_send(
                room_id,
                AnyMessageEventContent::RoomMessage(MessageEventContent::Text(
                    // TODO: Make a proper Message Creator for this
                    TextMessageEventContent::plain(message.as_str()),
                )),
                None,
            )
            .await
            .map_err(|err| err.into())
            .map(|_| ())
    }
     */
    async fn leave_room(&self, room_id: &RoomId) -> Result<()> {
        self.client
            .leave_room(room_id)
            .await
            .map_err(|err| err.into())
            .map(|_| ())
    }
}

pub trait EventExtract {
    fn sender(&self) -> &UserId;
    fn message(&self) -> Result<String>;
}

impl EventExtract for SyncMessageEvent<MessageEventContent> {
    fn sender(&self) -> &UserId {
        &self.sender
    }
    fn message(&self) -> Result<String> {
        match self {
            SyncMessageEvent {
                content: MessageEventContent::Text(TextMessageEventContent { body, .. }),
                ..
            } => Ok(body.to_owned()),
            _ => Err(anyhow!("not a string body")),
        }
    }
}

pub struct MatrixHandler {}

impl MatrixHandler {
    async fn handle_incoming_messages<T: EventExtract>(
        &self,
        room: SyncRoom,
        event: &T,
    ) -> Result<()> {
        debug!("Reacting to received message");

        if let SyncRoom::Joined(room) = room {
            // TODO
        } else {
            warn!("Received an message from an un-joined room");
        }

        Ok(())
    }
}

/*
#[async_trait]
impl EventEmitter for MatrixHandler {
    async fn on_room_message(&self, room: SyncRoom, event: &SyncMessageEvent<MessageEventContent>) {
        let _ = self
            .handle_incoming_messages::<SyncMessageEvent<MessageEventContent>>(room, event)
            .await
            .map_err(|err| {
                error!("{}", err);
            });
    }
}
*/