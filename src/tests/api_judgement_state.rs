use super::*;
use crate::actors::api::{JsonResult, NotifyAccountState, ResponseAccountState};
use crate::database::Database;
use crate::primitives::{IdentityContext, JudgementState};
use actix_http::ws::Frame;
use futures::{FutureExt, SinkExt, StreamExt};

#[actix::test]
async fn current_judgement_state_single_identity() {
    let (db, mut api, _) = new_env().await;

    let mut alice = JudgementState::alice();
    db.add_judgement_request(alice.clone()).await.unwrap();

    let mut stream = api.ws_at("/api/account_status").await.unwrap();
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

    alice.blank_second_challenge();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    wait().await;
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn current_judgement_state_multiple_inserts() {
    let (db, mut api, _) = new_env().await;

    let mut alice = JudgementState::alice();
    // Multiple inserts of the same request. Must not cause bad behavior.
    db.add_judgement_request(alice.clone()).await.unwrap();
    db.add_judgement_request(alice.clone()).await.unwrap();

    let mut stream = api.ws_at("/api/account_status").await.unwrap();
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

    alice.blank_second_challenge();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    wait().await;
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn current_judgement_state_multiple_identities() {
    let (db, mut api, _) = new_env().await;

    let mut alice = JudgementState::alice();
    let mut bob = JudgementState::bob();
    db.add_judgement_request(alice.clone()).await.unwrap();
    db.add_judgement_request(bob.clone()).await.unwrap();

    let mut stream = api.ws_at("/api/account_status").await.unwrap();
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

    alice.blank_second_challenge();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    stream.send(IdentityContext::bob().to_ws()).await.unwrap();
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

    bob.blank_second_challenge();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob))
    );

    wait().await;
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_field() {
    let (db, mut api, injector) = new_env().await;

    let mut alice = JudgementState::alice();
    db.add_judgement_request(alice.clone()).await.unwrap();

    let mut stream = api.ws_at("/api/account_status").await.unwrap();
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

    alice.blank_second_challenge();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    wait().await;
    assert!(stream.next().now_or_never().is_none());
}
