use crate::comms::{CommsMessage, CommsVerifier};
use crate::db::Database;
use crate::manager::AccountStatus;
use crate::primitives::{Account, AccountType, NetAccount, Result};
use crate::verifier::{invalid_accounts_message, verification_handler, Verifier, VerifierMessage};
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

#[derive(Debug, Fail)]
pub enum MatrixError {
    #[fail(display = "failed to open state store: {}", 0)]
    StateStore(failure::Error),
    #[fail(display = "failed to create client with the given config: {}", 0)]
    ClientCreation(failure::Error),
    #[fail(display = "failed to login into the homeserver: {}", 0)]
    Login(failure::Error),
    #[fail(display = "failed to sync: {}", 0)]
    Sync(failure::Error),
    #[fail(display = "the specified UserId is invalid: {}", 0)]
    InvalidUserId(failure::Error),
    #[fail(display = "failed to join room: {}", 0)]
    JoinRoom(failure::Error),
    #[fail(display = "timeout while trying to join room with: {}", 0)]
    JoinRoomTimeout(Account),
    #[fail(display = "the remote UserId was not found when trying to respond")]
    RemoteUserIdNotFound,
    #[fail(display = "failed to send message: {}", 0)]
    SendMessage(failure::Error),
    #[fail(display = "database error occured: {}", 0)]
    // TODO: Use `DatabaseError`
    Database(failure::Error),
    #[fail(
        display = "Failed to fetch challenge data from database for account: {:?}",
        0
    )]
    ChallengeDataNotFound(Account),
}

#[async_trait]
pub trait MatrixTransport: 'static + Send + Sync {
    async fn send_message(&self, room_id: &RoomId, message: VerifierMessage) -> Result<()>;
    async fn create_room<'a>(&'a self, request: Request<'a>) -> Result<Response>;
    async fn leave_room(&self, room_id: &RoomId) -> Result<()>;
    async fn user_id(&self) -> Result<UserId>;
    async fn run_emitter(&mut self, db: Database, comms: CommsVerifier);
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
        db: Database,
    ) -> Result<MatrixClient> {
        info!("Setting up Matrix client");
        // Setup client
        let store = JsonStore::open(db_path).map_err(|err| MatrixError::StateStore(err.into()))?;
        let client_config = ClientConfig::new().state_store(Box::new(store));

        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let client = Client::new_with_config(homeserver, client_config)
            .map_err(|err| MatrixError::ClientCreation(err.into()))?;

        // Login with credentials
        client
            .login(username, password, None, Some("w3f-registrar-bot"))
            .await
            .map_err(|err| MatrixError::Login(err.into()))?;

        // Sync up, avoid responding to old messages.
        info!("Syncing Matrix client");
        client
            .sync(SyncSettings::default())
            .await
            .map_err(|err| MatrixError::Sync(err.into()))?;

        // Request a list of open/pending room ids. Used to detect dead rooms.
        let pending_room_ids = db.select_room_ids().await?;

        // Leave dead rooms.
        info!("Detecting dead Matrix rooms");
        let rooms = client.joined_rooms();
        let rooms = rooms.read().await;
        for (room_id, _) in rooms.iter() {
            if pending_room_ids.iter().find(|&id| id == room_id).is_none() {
                warn!("Leaving dead room: {}", room_id.as_str());
                let _ = client.leave_room(room_id).await;
            }
        }

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

#[async_trait]
impl MatrixTransport for MatrixClient {
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
    async fn create_room<'a>(&'a self, request: Request<'a>) -> Result<Response> {
        self.client
            .create_room(request)
            .await
            .map_err(|err| err.into())
    }
    async fn leave_room(&self, room_id: &RoomId) -> Result<()> {
        self.client
            .leave_room(room_id)
            .await
            .map_err(|err| err.into())
            .map(|_| ())
    }
    async fn user_id(&self) -> Result<UserId> {
        //self.client.user_id().await.ok_or(failure::Error::from(Err(MatrixError::RemoteUserIdNotFound)))
        // TODO
        Ok(self.client.user_id().await.unwrap())
    }
    async fn run_emitter(&mut self, db: Database, comms: CommsVerifier) {
        // Add event emitter
        self.client
            .add_event_emitter(Box::new(MatrixHandler::new(db, comms, self.clone())))
            .await;
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
            _ => Err(failure::err_msg("not a string body")),
        }
    }
}

pub struct MatrixHandler {
    db: Database,
    comms: CommsVerifier,
    transport: Box<dyn MatrixTransport>,
}

impl MatrixHandler {
    pub fn new<T: 'static + MatrixTransport>(
        db: Database,
        comms: CommsVerifier,
        transport: T,
    ) -> Self {
        MatrixHandler {
            db: db,
            comms: comms,
            transport: Box::new(transport),
        }
    }
    pub async fn start(self) {
        loop {
            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
                err
            });
        }
    }
    async fn local(&self) -> Result<()> {
        use CommsMessage::*;

        match self.comms.recv().await {
            AccountToVerify {
                net_account,
                account,
            } => {
                self.handle_account_verification(net_account, account)
                    .await?
            }
            LeaveRoom { net_account } => {
                if let Some(room_id) = self.db.select_room_id(&net_account).await? {
                    self.transport
                        .send_message(&room_id, VerifierMessage::Goodbye("Bye bye!".to_string()))
                        .await?;
                    debug!("Leaving room: {}", room_id.as_str());
                    let _ = self.transport.leave_room(&room_id).await;
                } else {
                    debug!(
                        "No active Matrix room found for address {}",
                        net_account.as_str()
                    );
                }
            }
            NotifyInvalidAccount {
                net_account,
                account,
                accounts,
            } => {
                self.handle_invalid_account_notification(net_account, account, accounts)
                    .await?
            }
            #[cfg(test)]
            TriggerMatrixEmitter {
                room_id,
                my_user_id,
                event,
            } => {
                use matrix_sdk::{locks::RwLock, Room};
                use std::sync::Arc;

                let room_state =
                    SyncRoom::Joined(Arc::new(RwLock::new(Room::new(&room_id, &my_user_id))));
                self.handle_incoming_messages(room_state, &event).await?
            }
            _ => error!("Received unrecognized message type"),
        }

        Ok(())
    }
    async fn init_room_id(&self, net_account: &NetAccount, account: &Account) -> Result<RoomId> {
        // If a room already exists, don't create a new one.
        let room_id = if let Some(room_id) = self.db.select_room_id(&net_account).await? {
            room_id
        } else {
            // When the UserId is invalid, even though it can be successfully
            // converted, creating a room seems to block forever here. So we
            // just set a timeout and abort if exceeded.
            if let Ok(room_id) = time::timeout(Duration::from_secs(20), async {
                debug!("Connecting to {}", account.as_str());

                let to_invite = [account
                    .as_str()
                    .clone()
                    .try_into()
                    .map_err(|err| MatrixError::InvalidUserId(failure::Error::from(err)))?];

                let mut request = Request::default();
                request.invite = &to_invite;
                request.name = Some("W3F Registrar Verification");
                request.visibility = Visibility::Private;

                let resp = self
                    .transport
                    .create_room(request)
                    .await
                    .map_err(|err| MatrixError::JoinRoom(err.into()))?;

                debug!("Connection to user established");

                // Keep track of RoomId
                self.db
                    .insert_room_id(&net_account, &resp.room_id)
                    .await
                    .map_err(|err| MatrixError::Database(err.into()))?;

                StdResult::<_, MatrixError>::Ok(resp.room_id)
            })
            .await
            {
                // Mark the account as valid.
                self.db
                    .set_account_status(&net_account, &AccountType::Matrix, &AccountStatus::Valid)
                    .await?;

                room_id?
            } else {
                debug!("Failed to connect to account: {}", account.as_str());

                // Mark the account as invalid.
                self.db
                    .set_account_status(&net_account, &AccountType::Matrix, &AccountStatus::Invalid)
                    .await?;

                return Err(MatrixError::JoinRoomTimeout(account.clone()))?;
            }
        };

        Ok(room_id)
    }
    async fn handle_account_verification(
        &self,
        net_account: NetAccount,
        account: Account,
    ) -> Result<()> {
        let room_id = self.init_room_id(&net_account, &account).await?;

        let challenge_data = self
            .db
            .select_challenge_data(&account, &AccountType::Matrix)
            .await?;

        debug!("Sending instructions to user");
        let verifier = Verifier::new(&challenge_data);
        self.transport
            .send_message(&room_id, verifier.init_message_builder(true))
            .await
            .map_err(|err| MatrixError::SendMessage(err.into()).into())
            .map(|_| ())
    }
    async fn handle_incoming_messages<T: EventExtract>(
        &self,
        room: SyncRoom,
        event: &T,
    ) -> Result<()> {
        // Do not respond to its own messages.
        if event.sender()
            == &self
                .transport
                .user_id()
                .await
                .map_err(|_| MatrixError::RemoteUserIdNotFound)?
        {
            return Ok(());
        }

        debug!("Reacting to received message");

        if let SyncRoom::Joined(room) = room {
            debug!("Search for address based on RoomId");

            let room_id = &room.read().await.room_id;
            let account = Account::from(event.sender().as_str());

            debug!("Fetching challenge data");
            let challenge_data = self
                .db
                .select_challenge_data(&account, &AccountType::Matrix)
                .await?;

            if challenge_data.is_empty() {
                warn!("No challenge data found for {}", account.as_str());
                return Err(MatrixError::ChallengeDataNotFound(account.clone()).into());
            }

            let mut verifier = Verifier::new(&challenge_data);

            // Fetch the text message from the event.
            let msg_body = if let Ok(msg_body) = event.message() {
                msg_body
            } else {
                debug!(
                    "Didn't receive a text message from {}, notifying...",
                    event.sender().as_str()
                );

                self.transport
                    .send_message(
                        room_id,
                        VerifierMessage::InvalidFormat(
                            "Please send the signature directly as a text message.".to_string(),
                        ),
                    )
                    .await
                    .map_err(|err| MatrixError::SendMessage(err.into()))?;

                return Ok(());
            };

            debug!("Verifying message: {}", msg_body);
            verifier.verify(&msg_body);

            // Update challenge statuses and notify manager
            verification_handler(&verifier, &self.db, &self.comms, &AccountType::Matrix).await?;

            // Inform user about the current state of the verification
            self.transport
                .send_message(room_id, verifier.response_message_builder())
                .await
                .map_err(|err| MatrixError::SendMessage(err.into()))?;
        } else {
            warn!("Received an message from an un-joined room");
        }

        Ok(())
    }
    async fn handle_invalid_account_notification(
        &self,
        net_account: NetAccount,
        account: Account,
        accounts: Vec<(AccountType, Account)>,
    ) -> Result<()> {
        let room_id = self.init_room_id(&net_account, &account).await?;

        // Check for any display name violations (optional).
        let violations = self.db.select_display_name_violations(&net_account).await?;

        self.transport
            .send_message(&room_id, invalid_accounts_message(&accounts, violations))
            .await
            .map_err(|err| MatrixError::SendMessage(err.into()))?;

        Ok(())
    }
}

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
