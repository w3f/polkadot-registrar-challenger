use crate::comms::{CommsMessage, CommsVerifier};
use crate::db::Database2;
use crate::identity::AccountStatus;
use crate::primitives::{Account, AccountType, ChallengeStatus, NetAccount, Result};
use crate::verifier::Verifier2;
use matrix_sdk::{
    self,
    api::r0::room::create_room::Request,
    api::r0::room::Visibility,
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnyMessageEventContent, SyncMessageEvent,
    },
    identifiers::RoomId,
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
    #[fail(display = "contacted by a user who's RoomId was not registered anywhere")]
    RoomIdNotFound,
    #[fail(
        display = "Failed to fetch challenge data from database for account: {:?}",
        0
    )]
    ChallengeDataNotFound(Account),
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

#[derive(Clone)]
pub struct MatrixClient {
    client: Client, // `Client` from matrix_sdk
    comms: CommsVerifier,
    db: Database2,
}

impl MatrixClient {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        db_path: &str,
        db: Database2,
        comms: CommsVerifier,
        comms_emmiter: CommsVerifier,
    ) -> Result<MatrixClient> {
        info!("Setting up Matrix client");
        // Setup client
        let store = JsonStore::open(db_path).map_err(|err| MatrixError::StateStore(err.into()))?;
        let client_config = ClientConfig::new().state_store(Box::new(store));

        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let mut client = Client::new_with_config(homeserver, client_config)
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

        // Add event emitter (responder)
        client
            .add_event_emitter(Box::new(
                Responder::new(client.clone(), db.clone(), comms_emmiter).await,
            ))
            .await;

        let sync_client = client.clone();
        tokio::spawn(async move {
            sync_client
                .sync_forever(SyncSettings::default(), |_| async {})
                .await;
        });

        Ok(MatrixClient {
            client: client,
            comms: comms,
            db: db,
        })
    }
    async fn send_msg(&self, msg: &str, room_id: &RoomId) -> Result<()> {
        send_msg(&self.client, msg, room_id).await
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
            } => self.handle_account_verification(net_account, account).await,
            LeaveRoom { net_account } => {
                if let Some(room_id) = self.db.select_room_id(&net_account).await? {
                    self.send_msg("Bye bye!", &room_id).await?;
                    debug!("Leaving room: {}", room_id.as_str());
                    let _ = self.client.leave_room(&room_id).await?;
                } else {
                    debug!(
                        "No active Matrix room found for address {}",
                        net_account.as_str()
                    );
                }

                Ok(())
            }
            _ => panic!(),
        }
    }
    async fn handle_account_verification(
        &self,
        net_account: NetAccount,
        account: Account,
    ) -> Result<()> {
        // If a room already exists, don't create a new one.
        let room_id = if let Some(room_id) = self.db.select_room_id(&net_account).await? {
            room_id
        } else {
            // When the UserId is invalid, even though it can be successfully
            // converted, creating a room seems to block forever here. So we
            // just set a timeout and abort if exceeded.
            if let Ok(room_id) = time::timeout(Duration::from_secs(15), async {
                debug!("Connecting to {}", account.as_str());

                let to_invite = &[account
                    .as_str()
                    .clone()
                    .try_into()
                    .map_err(|err| MatrixError::InvalidUserId(failure::Error::from(err)))?];

                let mut request = Request::default();
                request.invite = to_invite;
                request.name = Some("W3F Registrar Verification");
                request.visibility = Visibility::Private;

                let resp = self
                    .client
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
                room_id?
            } else {
                debug!("Failed to connect to account: {}", account.as_str());

                // Mark the account as valid.
                self.db
                    .set_account_status(&net_account, &AccountType::Matrix, AccountStatus::Invalid)
                    .await?;

                return Err(MatrixError::JoinRoomTimeout(account.clone()))?;
            }
        };

        // Mark the account as invalid.
        self.db
            .set_account_status(&net_account, &AccountType::Matrix, AccountStatus::Valid)
            .await?;

        let challenge_data = self
            .db
            .select_challenge_data(&account, &AccountType::Matrix)
            .await?;

        debug!("Sending instructions to user");
        let verifier = Verifier2::new(&challenge_data);
        self.send_msg(&verifier.init_message_builder(true), &room_id)
            .await
            .map_err(|err| MatrixError::SendMessage(err.into()).into())
            .map(|_| ())
    }
}

struct Responder {
    client: Client,
    comms: CommsVerifier,
    db: Database2,
}

impl Responder {
    async fn new(client: Client, db: Database2, comms: CommsVerifier) -> Self {
        Responder {
            client: client,
            comms: comms,
            db: db,
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
        // Do not respond to its own messages.
        if event.sender
            == self
                .client
                .user_id()
                .await
                .ok_or(MatrixError::RemoteUserIdNotFound)?
        {
            return Ok(());
        }

        debug!("Reacting to received message");

        if let SyncRoom::Joined(room) = room {
            debug!("Search for address based on RoomId");

            let room_id = &room.read().await.room_id;
            let net_account = if let Some(net_account) =
                self.db.select_net_account_from_room_id(&room_id).await?
            {
                net_account
            } else {
                return Err(MatrixError::RoomIdNotFound.into());
            };

            let account = Account::from(event.sender.as_str());

            debug!("Fetching challenge data");
            let challenge_data = self
                .db
                .select_challenge_data(&account, &AccountType::Matrix)
                .await?;

            if challenge_data.is_empty() {
                warn!("No challenge data found for {}", account.as_str());
                return Err(MatrixError::ChallengeDataNotFound(account.clone()).into());
            }

            let mut verifier = Verifier2::new(&challenge_data);

            // Fetch the text message from the event.
            let msg_body = if let SyncMessageEvent {
                content: MessageEventContent::Text(TextMessageEventContent { body: msg_body, .. }),
                ..
            } = event
            {
                msg_body
            } else {
                debug!(
                    "Didn't receive a text message from {}, notifying...",
                    event.sender.as_str()
                );

                self.send_msg(
                    "Please send the signature directly as a text message.",
                    room_id,
                )
                .await
                .map_err(|err| MatrixError::SendMessage(err.into()))?;

                return Ok(());
            };

            debug!("Verifying message: {}", msg_body);
            verifier.verify(msg_body);

            for network_address in verifier.valid_verifications() {
                debug!(
                    "Valid verification for address: {}",
                    network_address.address().as_str()
                );

                self.comms
                    .notify_status_change(network_address.address().clone());

                self.db
                    .set_challenge_status(
                        network_address.address(),
                        &AccountType::Matrix,
                        ChallengeStatus::Accepted,
                    )
                    .await?;
            }

            for network_address in verifier.invalid_verifications() {
                debug!(
                    "Invalid verification for address: {}",
                    network_address.address().as_str()
                );

                self.db
                    .set_challenge_status(
                        network_address.address(),
                        &AccountType::Matrix,
                        ChallengeStatus::Rejected,
                    )
                    .await?;
            }

            self.send_msg(&verifier.response_message_builder(), room_id)
                .await
                .map_err(|err| MatrixError::SendMessage(err.into()))?;

            // Tell the manager to check the user's account states.
            self.comms.notify_status_change(net_account.clone());
        } else {
            warn!("Received an message from an un-joined room");
        }

        Ok(())
    }
}

#[async_trait]
impl EventEmitter for Responder {
    async fn on_room_message(&self, room: SyncRoom, event: &SyncMessageEvent<MessageEventContent>) {
        let _ = self.local(room, event).await.map_err(|err| {
            error!("{}", err);
        });
    }
}
