use crate::adapters::admin::{process_admin, Command, Response};
use crate::adapters::Adapter;
use crate::primitives::{ExternalMessage, ExternalMessageType, Timestamp};
use crate::{Database, Result};
use matrix_sdk::events::room::member::MemberEventContent;
use matrix_sdk::events::room::message::MessageEventContent;
use matrix_sdk::events::{AnyMessageEventContent, StrippedStateEvent, SyncMessageEvent};
use matrix_sdk::room::Room;
use matrix_sdk::{Client, ClientConfig, EventHandler, SyncSettings};
use ruma::events::room::message::{MessageType, TextMessageEventContent};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};
use url::Url;

const REJOIN_DELAY: u64 = 3;
const REJOIN_MAX_ATTEMPTS: usize = 5;

#[derive(Clone)]
pub struct MatrixClient {
    messages: Arc<Mutex<Vec<ExternalMessage>>>,
}

// TODO: Change workflow to chain methods.
impl MatrixClient {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        db_path: &str,
        db: Database,
        admins: Vec<MatrixHandle>,
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
                db,
                admins,
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

        actix::spawn(async move {
            client.clone().sync(settings).await;
        });

        Ok(MatrixClient { messages: messages })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixHandle(String);

struct Listener {
    client: Client,
    // TODO: Should probably just be a mpsc channel.
    messages: Arc<Mutex<Vec<ExternalMessage>>>,
    db: Database,
    admins: Vec<MatrixHandle>,
}

impl Listener {
    pub fn new(
        client: Client,
        messages: Arc<Mutex<Vec<ExternalMessage>>>,
        db: Database,
        admins: Vec<MatrixHandle>,
    ) -> Self {
        Self {
            client: client,
            messages: messages,
            db: db,
            admins: admins,
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
        if let Room::Joined(room) = room {
            let msg_body = if let SyncMessageEvent {
                content:
                    MessageEventContent {
                        msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
                        ..
                    },
                ..
            } = event
            {
                msg_body
            } else {
                debug!("Received unacceptable message type from {}", event.sender);
                return;
            };

            // Check for admin message
            let sender = event.sender.to_string();
            if self.admins.contains(&MatrixHandle(sender)) {
                let resp = match Command::from_str(&msg_body) {
                    // If a valid admin command was found, execute it.
                    Ok(cmd) => Some(process_admin(&self.db, cmd).await),
                    Err(err @ Response::InvalidSyntax(_)) => Some(err),
                    // Ignore, allow noise (catches `UnknownCommand`).
                    Err(_) => None,
                };

                // If response should be sent, then do so.
                if let Some(resp) = resp {
                    if let Err(err) = room
                        .send(
                            AnyMessageEventContent::RoomMessage(MessageEventContent::text_plain(
                                resp.to_string(),
                            )),
                            None,
                        )
                        .await
                    {
                        error!("Failed to send message: {:?}", err);
                    }
                }
            }

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
                values: vec![msg_body.to_string().into()],
            });
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
