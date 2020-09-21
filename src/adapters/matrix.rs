use crate::comms::{CommsMessage, CommsVerifier};
use crate::identity::AccountStatus;
use crate::primitives::{Account, AccountType, ChallengeStatus, Result};
use crate::verifier::Verifier;
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
    #[fail(display = "joining room timed out")]
    JoinRoomTimeout,
    #[fail(display = "the remote UserId was not found when trying to respond")]
    RemoteUserIdNotFound,
    #[fail(display = "the UserId is unknown (no pending on-chain judgement request)")]
    UnknownUser,
    #[fail(display = "failed to send message: {}", 0)]
    SendMessage(failure::Error),
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
}

impl MatrixClient {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        comms: CommsVerifier,
        comms_emmiter: CommsVerifier,
    ) -> Result<MatrixClient> {
        // Setup client
        let store = JsonStore::open("/tmp/matrix_store")
            .map_err(|err| MatrixError::StateStore(err.into()))?;
        let client_config = ClientConfig::new().state_store(Box::new(store));

        let homeserver = Url::parse(homeserver).expect("Couldn't parse the homeserver URL");
        let mut client = Client::new_with_config(homeserver, client_config)
            .map_err(|err| MatrixError::ClientCreation(err.into()))?;

        // Login with credentials
        client
            .login(username, password, None, Some("rust-sdk"))
            .await
            .map_err(|err| MatrixError::Login(err.into()))?;

        // Sync up, avoid responding to old messages.
        client
            .sync(SyncSettings::default())
            .await
            .map_err(|err| MatrixError::Sync(err.into()))?;

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

        Ok(MatrixClient {
            client: client,
            comms: comms,
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
        let (network_address, account, challenge, room_id) = self.comms.recv_inform().await;

        // If a room already exists, don't create a new one.
        let room_id = if let Some(room_id) = room_id {
            room_id
        } else {
            // When the UserId is invalid, even though it can be successfully
            // converted, creating a room seems to block forever here. So we
            // just set a timeout and abort if exceeded.
            if let Ok(room_id) = time::timeout(Duration::from_secs(15), async {
                debug!("Connecting to {}", account.as_str());

                let to_invite = [account
                    .as_str()
                    .clone()
                    .try_into()
                    .map_err(|err| MatrixError::InvalidUserId(failure::Error::from(err)))?];

                let mut request = Request::default();
                request.invite = &to_invite;
                request.name = Some("W3F Registrar Verification");

                let resp = self
                    .client
                    .create_room(request)
                    .await
                    .map_err(|err| MatrixError::JoinRoom(err.into()))?;

                self.comms
                    .notify_room_id(network_address.address().clone(), resp.room_id.clone());

                StdResult::<_, MatrixError>::Ok(resp.room_id)
            })
            .await
            {
                room_id?
            } else {
                return Err(MatrixError::JoinRoomTimeout)?;
            }
        };

        // Notify that the account is valid.
        self.comms.notify_account_status(
            network_address.clone(),
            AccountType::Matrix,
            AccountStatus::Valid,
        );

        // Send the instructions for verification to the user.
        self.send_msg(
            include_str!("../../messages/instructions")
                .replace("{:PAYLOAD}", &challenge.as_str())
                .replace("{:ADDRESS}", network_address.address().as_str())
                .as_str(),
            &room_id,
        )
        .await
        .map_err(|err| MatrixError::SendMessage(err.into()))?;

        Ok(())
    }
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

        if let SyncRoom::Joined(room) = room {
            // Request information about the sender.
            self.comms.notify_account_state_request(
                Account::from(event.sender.as_str().to_string()),
                AccountType::Matrix,
            );

            let room_id = &room.read().await.room_id;

            let (network_address, challenge) = match self.comms.recv().await {
                CommsMessage::AccountToVerify {
                    network_address,
                    challenge,
                    ..
                } => (network_address, challenge),
                CommsMessage::InvalidRequest => {
                    // Reject user
                    return Err(failure::Error::from(MatrixError::UnknownUser));
                }
                _ => panic!("Received unrecognized message type on Matrix client. Report as bug."),
            };

            let verifier = Verifier::new(network_address.clone(), challenge);

            // Fetch the text message from the event.
            let msg_body = if let SyncMessageEvent {
                content: MessageEventContent::Text(TextMessageEventContent { body: msg_body, .. }),
                ..
            } = event
            {
                msg_body
            } else {
                debug!(
                    "Didn't receive a text message from {}",
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

            match verifier.verify(&msg_body) {
                Ok(msg) => {
                    self.send_msg(&msg, room_id)
                        .await
                        .map_err(|err| MatrixError::SendMessage(err.into()))?;

                    self.comms.notify_challenge_status(
                        network_address,
                        AccountType::Matrix,
                        ChallengeStatus::Accepted,
                    );
                }
                Err(err) => {
                    self.send_msg(&err.to_string(), room_id)
                        .await
                        .map_err(|err| MatrixError::SendMessage(err.into()))?;

                    self.comms.notify_challenge_status(
                        network_address,
                        AccountType::Matrix,
                        ChallengeStatus::Rejected,
                    );
                }
            };
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
