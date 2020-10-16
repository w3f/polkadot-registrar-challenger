use super::{MatrixTransport, EmailTransport, TwitterTransport};
use super::email;
use super::twitter::{self, TwitterId, TwitterError};
use crate::{Database2, Account};
use crate::comms::CommsVerifier;
use crate::primitives::{Result, NetAccount, unix_time};
use crate::manager::OnChainIdentity;
use matrix_sdk::identifiers::{RoomId, UserId};
use matrix_sdk::api::r0::room::create_room::{Request, Response};
use tokio::sync::RwLock;
use std::result::Result as StdResult;
use std::collections::HashMap;
use std::convert::TryFrom;

enum Event {
    Matrix(MatrixEvent),
}

enum MatrixEvent {
    SendMessage {
        room_id: RoomId,
        message: String,
    },
    CreateRoom {
        user_id: UserId,
    },
    LeaveRoom {
        room_id: RoomId,
    }
}

struct EventManager {
    events: RwLock<Vec<Event>>,
}

impl EventManager {
    fn new() -> Self {
        EventManager {
            events: RwLock::new(vec![]),
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
        self.events.push(Event::Matrix(MatrixEvent::CreateRoom {
            user_id: request.invite[0].clone()
        }));

        Ok(Response::new(RoomId::try_from(format!("!{}:matrix.org", unix_time()).as_str()).unwrap()))
    }
    async fn leave_room(&self, room_id: &RoomId) -> Result<()> {
        self.events.push(Event::Matrix(MatrixEvent::LeaveRoom {
            room_id: room_id.clone(),
        }));

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

}

#[async_trait]
impl EmailTransport for EmailMocker {
    async fn request_messages(&self) -> Result<Vec<email::ReceivedMessageContext>> {
        unimplemented!()
    }
    async fn send_message(&self, account: &Account, msg: String) -> Result<()> {
        unimplemented!()
    }
}

pub struct TwitterMocker {

}

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
