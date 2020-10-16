use super::email;
use super::twitter::{self, TwitterError, TwitterId};
use super::{EmailTransport, MatrixTransport, TwitterTransport};
use crate::comms::CommsVerifier;
use crate::manager::OnChainIdentity;
use crate::primitives::{unix_time, NetAccount, Result};
use crate::{Account, Database2};
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
}

enum MatrixEvent {
    SendMessage { room_id: RoomId, message: String },
    CreateRoom { user_id: UserId },
    LeaveRoom { room_id: RoomId },
}

enum EmailEvent {
    RequestMessages(Vec<email::ReceivedMessageContext>),
    SendMessage {
        account: Account,
        message: String,
    }
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

#[async_trait]
impl MatrixTransport for MatrixMocker {
    async fn send_message(&self, room_id: &RoomId, message: String) -> Result<()> {
        self.events.push(Event::Matrix(MatrixEvent::SendMessage {
            room_id: room_id.clone(),
            message: message,
        }));

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
    messages: Arc<RwLock<Vec<email::ReceivedMessageContext>>>,
}

#[async_trait]
impl EmailTransport for EmailMocker {
    async fn request_messages(&self) -> Result<Vec<email::ReceivedMessageContext>> {
        self.events.push(Event::Email(EmailEvent::RequestMessages(
            self.messages.read().await.clone(),
        ))).await;

        let messages = self.messages.read().await;
        self.messages.write().await.clear();

        Ok(messages.to_vec())
    }
    async fn send_message(&self, account: &Account, msg: String) -> Result<()> {
        self.events.push(Event::Email(EmailEvent::SendMessage {
            account: account.clone(),
            message: msg,
        })).await;

        Ok(())
    }
}

pub struct TwitterMocker {}

#[async_trait]
impl TwitterTransport for TwitterMocker {
    async fn request_messages(
        &self,
        exclude: &TwitterId,
        watermark: u64,
    ) -> Result<(Vec<twitter::ReceivedMessageContext>, u64)> {
        unimplemented!()
    }
    async fn lookup_twitter_id(
        &self,
        twitter_ids: Option<&[&TwitterId]>,
        accounts: Option<&[&Account]>,
    ) -> Result<Vec<(Account, TwitterId)>> {
        unimplemented!()
    }
    async fn send_message(&self, id: &TwitterId, message: String) -> StdResult<(), TwitterError> {
        unimplemented!()
    }
    fn my_screen_name(&self) -> &Account {
        unimplemented!()
    }
}
