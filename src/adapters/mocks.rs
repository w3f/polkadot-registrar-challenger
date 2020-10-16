use super::{MatrixTransport, EmailTransport, TwitterTransport};
use super::email;
use super::twitter::{self, TwitterId, TwitterError};
use crate::{Database2, Account};
use crate::comms::CommsVerifier;
use crate::primitives::Result;
use matrix_sdk::identifiers::{RoomId, UserId};
use matrix_sdk::api::r0::room::create_room::{Request, Response};
use std::result::Result as StdResult;

pub struct MatrixMocker {

}

#[async_trait]
impl MatrixTransport for MatrixMocker {
    async fn send_message(&self, room_id: &RoomId, message: String) -> Result<()> {
        unimplemented!()
    }
    async fn create_room<'a>(&'a self, request: Request<'a>) -> Result<Response> {
        unimplemented!()
    }
    async fn leave_room(&self, room_id: &RoomId) -> Result<()> {
        unimplemented!()
    }
    async fn user_id(&self) -> Result<UserId> {
        unimplemented!()
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
