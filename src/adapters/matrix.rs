use crate::actors::verifier::Adapter;
use crate::primitives::{ExternalMessage, ExternalMessageType, MessageId, MessagePart, Timestamp};
use crate::Result;
use matrix_sdk::events::room::member::MemberEventContent;
use matrix_sdk::events::room::message::MessageEventContent;
use matrix_sdk::events::{StrippedStateEvent, SyncMessageEvent};
use matrix_sdk::{Client, ClientConfig, EventEmitter, RoomState, SyncSettings};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};
use url::Url;

const REJOIN_DELAY: u64 = 3;
const REJOIN_MAX_ATTEMPTS: usize = 5;

// TODO: This type should be unified with other adapters.
pub struct MatrixMessage {
    from: String,
    message: String,
}

#[derive(Clone)]
pub struct MatrixClient {
    client: Client, // `Client` from matrix_sdk
    messages: Arc<Mutex<Vec<ExternalMessage>>>,
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

        Ok(MatrixClient {
            client: client,
            messages: Arc::new(Mutex::new(vec![])),
        })
    }
    pub async fn start(&self) {
        self.client.add_event_emitter(Box::new(self.clone())).await;
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

                if rejoin_attempts == REJOIN_MAX_ATTEMPTS {
                    error!("Can't join room {} ({:?})", room.room_id(), err);
                    return;
                }

                delay *= 2;
                rejoin_attempts += 1;
            }

            debug!("Joined room {}", room.room_id());
        }
    }
    async fn on_room_message(
        &self,
        room: RoomState,
        event: &SyncMessageEvent<MessageEventContent>,
    ) {
        if let RoomState::Joined(_) = room {
            match &event.content {
                MessageEventContent::Text(content) => {
                    debug!(
                        "Received message \"{}\" from {}",
                        content.body, event.sender
                    );

                    // Add external message to inner field. That field is then
                    // fetched by the `Adapter` implementation.
                    let mut lock = self.messages.lock().await;
                    (*lock).push(ExternalMessage {
                        origin: ExternalMessageType::Matrix(event.sender.to_string()),
                        // A message UID is not relevant regarding a live
                        // message listener. The Matrix SDK handles
                        // synchronization.
                        id: 0u32.into(),
                        timestamp: Timestamp::now(),
                        values: vec![content.body.to_string().into()],
                    });
                }
                _ => {
                    trace!("Received unacceptable message type from {}", event.sender);
                }
            }
        }
    }
}

#[async_trait]
impl Adapter for MatrixClient {
    fn name(&self) -> &'static str {
        "Matrix"
    }
    async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>> {
        let mut lock = self.messages.lock().await;
        // Return messages and wipe inner field.
        Ok(std::mem::take(&mut *lock))
    }
}
