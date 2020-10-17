use super::email;
use super::twitter::{self, TwitterError, TwitterId};
use super::{EmailTransport, MatrixTransport, TwitterTransport};
use crate::comms::CommsVerifier;
use crate::manager::OnChainIdentity;
use crate::primitives::{unix_time, NetAccount, Result};
use crate::{Account, Database2};
use futures::stream::TryStreamExt;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender};
use matrix_sdk::api::r0::room::create_room::{Request, Response};
use matrix_sdk::identifiers::{RoomId, UserId};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::result::Result as StdResult;
use std::sync::Arc;
use tokio::sync::RwLock;

enum Event {
    Matrix(MatrixEvent),
    Email(EmailEvent),
    Twitter(TwitterEvent),
}

enum MatrixEvent {
    SendMessage { room_id: RoomId, message: String },
    CreateRoom { user_id: UserId },
    LeaveRoom { room_id: RoomId },
}

enum EmailEvent {
    RequestMessages(Vec<email::ReceivedMessageContext>),
    SendMessage { account: Account, message: String },
}

enum TwitterEvent {
    RequestMessages {
        exclude: TwitterId,
        watermark: u64,
        messages: Vec<twitter::ReceivedMessageContext>,
    },
    LookupTwitterId {
        twitter_ids: Option<Vec<TwitterId>>,
        accounts: Option<Vec<Account>>,
        lookups: Vec<(Account, TwitterId)>,
    },
    SendMessage {
        id: TwitterId,
        message: String,
    },
}

struct EventManager {
    events: Arc<RwLock<Vec<Event>>>,
}

impl EventManager {
    fn new() -> Self {
        EventManager {
            events: Arc::new(RwLock::new(vec![])),
        }
    }
    async fn push(&self, event: Event) {
        self.events.write().await.push(event);
    }
}

pub struct MatrixMocker {
    events: EventManager,
    user_id: UserId,
}

impl MatrixMocker {
    fn new(user_id: UserId) -> Self {
        MatrixMocker {
            events: EventManager::new(),
            user_id: user_id,
        }
    }
}

#[async_trait]
impl MatrixTransport for MatrixMocker {
    async fn send_message(&self, room_id: &RoomId, message: String) -> Result<()> {
        self.events
            .push(Event::Matrix(MatrixEvent::SendMessage {
                room_id: room_id.clone(),
                message: message,
            }))
            .await;

        Ok(())
    }
    async fn create_room<'a>(&'a self, request: Request<'a>) -> Result<Response> {
        self.events
            .push(Event::Matrix(MatrixEvent::CreateRoom {
                user_id: request.invite[0].clone(),
            }))
            .await;

        Ok(Response::new(
            RoomId::try_from(format!("!{}:matrix.org", unix_time()).as_str()).unwrap(),
        ))
    }
    async fn leave_room(&self, room_id: &RoomId) -> Result<()> {
        self.events
            .push(Event::Matrix(MatrixEvent::LeaveRoom {
                room_id: room_id.clone(),
            }))
            .await;

        Ok(())
    }
    async fn user_id(&self) -> Result<UserId> {
        Ok(self.user_id.clone())
    }
    async fn run_emitter(&mut self, db: Database2, comms: CommsVerifier) {
        unimplemented!()
    }
}

pub struct EmailMocker {
    events: EventManager,
    receiver: Arc<RwLock<UnboundedReceiver<Result<Vec<email::ReceivedMessageContext>>>>>,
}

impl EmailMocker {
    fn new(receiver: UnboundedReceiver<Result<Vec<email::ReceivedMessageContext>>>) -> Self {
        EmailMocker {
            events: EventManager::new(),
            receiver: Arc::new(RwLock::new(receiver)),
        }
    }
}

#[async_trait]
impl EmailTransport for EmailMocker {
    async fn request_messages(&self) -> Result<Vec<email::ReceivedMessageContext>> {
        let mut messages = if let Some(messages) =
            TryStreamExt::try_next(&mut *self.receiver.write().await)
                .await
                .unwrap()
        {
            messages
        } else {
            vec![]
        };

        self.events
            .push(Event::Email(EmailEvent::RequestMessages(messages.clone())))
            .await;

        Ok(messages)
    }
    async fn send_message(&self, account: &Account, msg: String) -> Result<()> {
        self.events
            .push(Event::Email(EmailEvent::SendMessage {
                account: account.clone(),
                message: msg,
            }))
            .await;

        Ok(())
    }
}

pub struct TwitterMocker {
    events: EventManager,
    receiver: Arc<RwLock<UnboundedReceiver<Result<Vec<twitter::ReceivedMessageContext>>>>>,
    index_book: Vec<(Account, TwitterId)>,
    screen_name: Account,
}

impl TwitterMocker {
    fn new(
        receiver: UnboundedReceiver<Result<Vec<twitter::ReceivedMessageContext>>>,
        screen_name: Account,
        index_book: Vec<(Account, TwitterId)>,
    ) -> Self {
        TwitterMocker {
            events: EventManager::new(),
            receiver: Arc::new(RwLock::new(receiver)),
            index_book: index_book,
            screen_name: screen_name,
        }
    }
}

#[async_trait]
impl TwitterTransport for TwitterMocker {
    async fn request_messages(
        &self,
        exclude: &TwitterId,
        watermark: u64,
    ) -> Result<(Vec<twitter::ReceivedMessageContext>, u64)> {
        let mut messages = if let Some(messages) =
            TryStreamExt::try_next(&mut *self.receiver.write().await)
                .await
                .unwrap()
        {
            messages
        } else {
            vec![]
        };

        let mut new_watermark = 0;
        let messages = messages.into_iter()
            .filter(|message| {
                if message.created > new_watermark {
                    new_watermark = message.created;
                }

                message.created > watermark
            })
            .collect::<Vec<twitter::ReceivedMessageContext>>();

        self.events
            .push(Event::Twitter(TwitterEvent::RequestMessages {
                exclude: exclude.clone(),
                watermark: watermark,
                messages: messages.clone(),
            }))
            .await;

        Ok((messages, new_watermark))
    }
    async fn lookup_twitter_id(
        &self,
        twitter_ids: Option<&[&TwitterId]>,
        accounts: Option<&[&Account]>,
    ) -> Result<Vec<(Account, TwitterId)>> {
        let mut lookups = vec![];

        if let Some(twitter_ids) = twitter_ids {
            for twitter_id in twitter_ids {
                let pair = self
                    .index_book
                    .iter()
                    .find(|(_, id)| id == *twitter_id)
                    .unwrap();

                lookups.push(pair.clone());
            }
        }

        if let Some(accounts) = accounts {
            for account in accounts {
                let pair = self
                    .index_book
                    .iter()
                    .find(|(acc, _)| acc == *account)
                    .unwrap();

                lookups.push(pair.clone());
            }
        }

        self.events
            .push(Event::Twitter(TwitterEvent::LookupTwitterId {
                twitter_ids: twitter_ids.map(|ids| ids.iter().map(|&id| id.clone()).collect()),
                accounts: accounts
                    .map(|accounts| accounts.iter().map(|&account| account.clone()).collect()),
                lookups: lookups.clone(),
            }))
            .await;

        Ok(lookups)
    }
    async fn send_message(&self, id: &TwitterId, message: String) -> StdResult<(), TwitterError> {
        self.events
            .push(Event::Twitter(TwitterEvent::SendMessage {
                id: id.clone(),
                message: message,
            }))
            .await;

        Ok(())
    }
    fn my_screen_name(&self) -> &Account {
        &self.screen_name
    }
}
