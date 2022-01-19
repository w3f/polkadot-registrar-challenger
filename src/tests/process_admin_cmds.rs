use super::*;
use crate::actors::api::VerifyChallenge;
use crate::actors::api::{JsonResult, ResponseAccountState};
use crate::adapters::admin::{process_admin, Command, RawFieldName, Response};
use crate::primitives::{
    ExpectedMessage, ExternalMessage, ExternalMessageType, IdentityContext, JudgementState,
    JudgementStateBlanked, MessageId, NotificationMessage, Timestamp,
};
use actix_http::StatusCode;
use futures::{FutureExt, SinkExt, StreamExt};

#[actix::test]
async fn command_status() {
    let (db, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    db.add_judgement_request(&alice).await.unwrap();

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Request status.
    let resp = process_admin(&db, Command::Status(alice.context.address.clone())).await;
    assert_eq!(resp, Response::Status(JudgementStateBlanked::from(alice)));

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_all() {
    let (db, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    db.add_judgement_request(&alice).await.unwrap();

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Request status.
    let resp = process_admin(
        &db,
        Command::Verify(
            alice.context.address.clone(),
            vec![RawFieldName::DisplayName],
        ),
    )
    .await;
    //assert_eq!(resp, Response::Status(JudgementStateBlanked::from(alice)));

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}
