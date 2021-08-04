use super::*;
use crate::actors::api::{JsonResult, ResponseAccountState};
use crate::primitives::{
    ExpectedMessage, ExternalMessage, ExternalMessageType, IdentityContext, IdentityFieldValue,
    JudgementState, MessageId, NotificationMessage, Timestamp,
};
use futures::{FutureExt, SinkExt, StreamExt};

#[actix::test]
async fn current_judgement_state_single_identity() {
    let (db, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    db.add_judgement_request(&alice).await.unwrap();

    wait().await;

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn current_judgement_state_multiple_inserts() {
    let (db, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    // Multiple inserts of the same request. Must not cause bad behavior.
    db.add_judgement_request(&alice).await.unwrap();
    db.add_judgement_request(&alice).await.unwrap();

    wait().await;

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn current_judgement_state_multiple_identities() {
    let (db, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    let bob = JudgementState::bob();
    db.add_judgement_request(&alice).await.unwrap();
    db.add_judgement_request(&bob).await.unwrap();

    wait().await;

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    // Check state of Bob
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_invalid_message_bad_challenge() {
    let (db, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let bob = JudgementState::bob();
    db.add_judgement_request(&alice).await.unwrap();
    db.add_judgement_request(&bob).await.unwrap();

    wait().await;

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Send invalid message (bad challenge).
    injector
        .send(ExternalMessage {
            origin: ExternalMessageType::Email("alice@email.com".to_string()),
            id: MessageId::from(0u32),
            timestamp: Timestamp::now(),
            values: ExpectedMessage::random().to_message_parts(),
        })
        .await;

    // The expected message (field verification failed).
    *alice
        .get_field_mut(&IdentityFieldValue::Email("alice@email.com".to_string()))
        .failed_attempts_mut() = 1;

    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerificationFailed {
            context: alice.context.clone(),
            field: IdentityFieldValue::Email("alice@email.com".to_string()),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Other judgement states must be unaffected.
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_invalid_message_bad_origin() {
    let (db, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    let bob = JudgementState::bob();
    db.add_judgement_request(&alice).await.unwrap();
    db.add_judgement_request(&bob).await.unwrap();

    wait().await;

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Send invalid message (bad origin).
    injector
        .send(ExternalMessage {
            origin: ExternalMessageType::Email("eve@email.com".to_string()),
            id: MessageId::from(0u32),
            timestamp: Timestamp::now(),
            values: alice
                .get_field(&IdentityFieldValue::Email("alice@email.com".to_string()))
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // No response is sent.
    assert!(stream.next().now_or_never().is_none());

    // Other judgement states must be unaffected.
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_valid_message() {
    let (db, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let bob = JudgementState::bob();
    db.add_judgement_request(&alice).await.unwrap();
    db.add_judgement_request(&bob).await.unwrap();

    wait().await;

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Send valid message.
    injector
        .send(ExternalMessage {
            origin: ExternalMessageType::Matrix("@alice:matrix.org".to_string()),
            id: MessageId::from(0u32),
            timestamp: Timestamp::now(),
            values: alice
                .get_field(&IdentityFieldValue::Matrix("@alice:matrix.org".to_string()))
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Matrix account of Alice is now verified
    alice
        .get_field_mut(&IdentityFieldValue::Matrix("@alice:matrix.org".to_string()))
        .expected_message_mut()
        .set_verified();

    // The expected message (field verified successfully).
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: IdentityFieldValue::Matrix("@alice:matrix.org".to_string()),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Other judgement states must be unaffected.
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}
