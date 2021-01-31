use crate::{event, Result};
use async_channel::Sender;
use matrix_sdk::events::room::member::MemberEventContent;
use matrix_sdk::events::room::message::MessageEventContent;
use matrix_sdk::events::{StrippedStateEvent, SyncMessageEvent};
use matrix_sdk::identifiers::{RoomId, UserId};
use matrix_sdk::{Client, ClientConfig, EventEmitter, RoomState, SyncSettings};
use std::convert::TryInto;
use std::result::Result as StdResult;
use tokio::time::{self, Duration};
use url::Url;

const REJOIN_DELAY: u64 = 3;
const REJOIN_MAX_ATTEMPTS: usize = 5;

pub struct MatrixMessage {
    from: String,
    message: String,
}

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
        sender: Sender<MatrixMessage>,
    ) -> Result<MatrixClient> {
        info!("Setting up Matrix client");
        // Setup client
        let client_config = ClientConfig::new().store_path(db_path);

        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let client = Client::new_with_config(homeserver, client_config)?;

        // Login with credentials
        client
            .login(username, password, None, Some("w3f-registrar-bot"))
            .await?;

        // Sync up, avoid responding to old messages.
        info!("Syncing Matrix client");
        client.sync(SyncSettings::default()).await;

        let matrix = MatrixClient { client: client };

        Ok(matrix)
    }
    async fn leave_room(&self, room_id: &RoomId) -> Result<()> {
        self.client
            .leave_room(room_id)
            .await
            .map_err(|err| err.into())
            .map(|_| ())
    }
}

#[async_trait]
impl EventEmitter for MatrixClient {
    async fn on_stripped_state_member(
        &self,
        room: RoomState,
        _: &StrippedStateEvent<MemberEventContent>,
        _: Option<MemberEventContent>,
    ) {
        if let RoomState::Invited(room) = room {
            let mut delay = REJOIN_DELAY;
            let mut rejoin_attempts = 0;

            while let Err(err) = self.client.join_room_by_id(room.room_id()).await {
                warn!(
                    "Failed to join room {} ({:?}), retrying in {}s",
                    room.room_id(),
                    err,
                    delay,
                );

                time::sleep(Duration::from_secs(delay)).await;
                delay *= 2;
                rejoin_attempts += 1;

                if rejoin_attempts == REJOIN_MAX_ATTEMPTS {
                    error!("Can't join room {} ({:?})", room.room_id(), err);
                    break;
                }
            }

            debug!("Joined room {}", room.room_id());
        }
    }
    async fn on_room_message(&self, _: RoomState, _: &SyncMessageEvent<MessageEventContent>) {}
}
