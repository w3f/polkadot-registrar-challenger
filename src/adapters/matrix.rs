use crate::adapters::Adapter;
use crate::primitives::{ExternalMessage, ExternalMessageType, Timestamp};
use crate::Result;
use matrix_sdk::events::room::member::MemberEventContent;
use matrix_sdk::events::room::message::MessageEventContent;
use matrix_sdk::events::{StrippedStateEvent, SyncMessageEvent};
use matrix_sdk::room::Room;
use matrix_sdk::{Client, ClientConfig, EventHandler, SyncSettings};
use ruma::events::room::message::MessageType;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};
use url::Url;

const REJOIN_DELAY: u64 = 3;
const REJOIN_MAX_ATTEMPTS: usize = 5;

#[derive(Clone)]
pub struct MatrixClient {
    client: Client,
    messages: Arc<Mutex<Vec<ExternalMessage>>>,
}

// TODO: Change workflow to chain methods.
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

        let homeserver = Url::parse(homeserver)?;
        let client = Client::new_with_config(homeserver, client_config)?;

        // Login with credentials
        info!("Login with credentials");
        client
            .login(username, password, None, Some("w3f-registrar-bot"))
            .await?;

        // Sync up, avoid responding to old messages.
        info!("Syncing client");
        client.sync_once(SyncSettings::default()).await?;

        // Add event handler
        let messages = Arc::new(Mutex::new(vec![]));
        client
            .set_event_handler(Box::new(Listener::new(
                client.clone(),
                Arc::clone(&messages),
            )))
            .await;

        // Start backend syncing service
        info!("Executing background sync");
        let settings = SyncSettings::default().token(
            client
                .sync_token()
                .await
                .ok_or(anyhow!("Failed to acquire sync token"))?,
        );

        let t_client = client.clone();
        actix::spawn(async move {
            t_client.clone().sync(settings).await;
        });

        Ok(MatrixClient {
            client: client,
            messages: messages,
        })
    }
}

struct Listener {
    client: Client,
    // TODO: Rename?
    messages: Arc<Mutex<Vec<ExternalMessage>>>,
}

impl Listener {
    pub fn new(client: Client, messages: Arc<Mutex<Vec<ExternalMessage>>>) -> Self {
        Self {
            client: client,
            messages: messages,
        }
    }
}

#[async_trait]
impl EventHandler for Listener {
    async fn on_stripped_state_member(
        &self,
        room: Room,
        _: &StrippedStateEvent<MemberEventContent>,
        _: Option<MemberEventContent>,
    ) {
        if let Room::Invited(room) = room {
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
    async fn on_room_message(&self, room: Room, event: &SyncMessageEvent<MessageEventContent>) {
        if let Room::Joined(_) = room {
            match &event.content.msgtype {
                MessageType::Text(content) => {
                    debug!("Received message from {}", event.sender);

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
                    debug!("Received unacceptable message type from {}", event.sender);
                }
            }
        }
    }
}

#[async_trait]
impl Adapter for MatrixClient {
    type MessageType = ();

    fn name(&self) -> &'static str {
        "Matrix"
    }
    async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>> {
        let mut lock = self.messages.lock().await;
        // Return messages and wipe inner field.
        Ok(std::mem::take(&mut *lock))
    }
    async fn send_message(&mut self, _to: &str, _content: Self::MessageType) -> Result<()> {
        unimplemented!()
    }
}
