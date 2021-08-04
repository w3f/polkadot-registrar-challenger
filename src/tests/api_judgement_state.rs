use super::*;
use crate::actors::api::VerifyChallenge;
use crate::actors::api::{JsonResult, ResponseAccountState};
use crate::primitives::{
    ExpectedMessage, ExternalMessage, ExternalMessageType, IdentityContext, IdentityFieldValue,
    JudgementState, MessageId, NotificationMessage, Timestamp,
};
use actix_http::StatusCode;
use futures::{FutureExt, SinkExt, StreamExt};

// Convenience type
type F = IdentityFieldValue;

#[actix::test]
async fn current_judgement_state_single_identity() {
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
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

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

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

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

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state (Alice).
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    // Check current state (Bob).
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

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
        .get_field_mut(&F::alice_email())
        .failed_attempts_mut() = 1;

    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerificationFailed {
            context: alice.context.clone(),
            field: F::alice_email(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(
        resp.state
            .get_field(&F::alice_email())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Other judgement states must be unaffected (Bob).
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

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
                .get_field(&F::alice_email())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // No response is sent. The service ignores unknown senders.
    assert!(stream.next().now_or_never().is_none());

    // Other judgement states must be unaffected (Bob).
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

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
                .get_field(&F::alice_matrix())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Matrix account of Alice is now verified
    alice
        .get_field_mut(&F::alice_matrix())
        .expected_message_mut()
        .set_verified();

    // The expected message (field verified successfully).
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: F::alice_matrix(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(
        resp.state
            .get_field(&F::alice_email())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::alice_twitter())
            .expected_message()
            .is_verified,
        false
    );
    // VERIFIED
    assert_eq!(
        resp.state
            .get_field(&F::alice_matrix())
            .expected_message()
            .is_verified,
        true
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Other judgement states must be unaffected (Bob).
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(
        resp.state
            .get_field(&F::bob_email())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::bob_twitter())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::bob_matrix())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_valid_message_duplicate_account_name() {
    let (db, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let mut bob = JudgementState::bob();

    // Bob also has the same Matrix account as Alice.
    bob.get_field_mut(&F::bob_matrix())
        .value = F::alice_matrix();

    db.add_judgement_request(&alice).await.unwrap();
    db.add_judgement_request(&bob).await.unwrap();

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
                .get_field(&F::alice_matrix())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Email account of Alice is now verified
    alice
        .get_field_mut(&F::alice_matrix())
        .expected_message_mut()
        .set_verified();

    // The expected message (field verified successfully).
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: F::alice_matrix(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(
        resp.state
            .get_field(&F::alice_email())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::alice_twitter())
            .expected_message()
            .is_verified,
        false
    );
    // VERIFIED
    assert_eq!(
        resp.state
            .get_field(&F::alice_matrix())
            .expected_message()
            .is_verified,
        true
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Other judgement states must be unaffected (Bob), but will receive a "failed attempt".
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    *bob.get_field_mut(&F::alice_matrix())
        .failed_attempts_mut() = 1;

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(
        resp.state
            .get_field(&F::bob_email())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::bob_twitter())
            .expected_message()
            .is_verified,
        false
    );
    // NOT VERIFIED, even though both have the same account specified.
    assert_eq!(
        resp.state
            .get_field(&F::alice_matrix())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_valid_message_awaiting_second_challenge() {
    let (db, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let bob = JudgementState::bob();

    db.add_judgement_request(&alice).await.unwrap();
    db.add_judgement_request(&bob).await.unwrap();

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
            origin: ExternalMessageType::Email("alice@email.com".to_string()),
            id: MessageId::from(0u32),
            timestamp: Timestamp::now(),
            values: alice
                .get_field(&F::alice_email())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Email account of Alice is now verified
    alice
        .get_field_mut(&F::alice_email())
        .expected_message_mut()
        .set_verified();

    // The expected message (field verified successfully).
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: F::alice_email(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Explicit tests.
    let resp = resp.unwrap();
    // VERIFIED
    assert_eq!(
        resp.state
            .get_field(&F::alice_email())
            .expected_message()
            .is_verified,
        true
    );
    // NOT VERIFIED. Second challenge is still required.
    assert_eq!(
        resp.state
            .get_field(&F::alice_email())
            .expected_second()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::alice_twitter())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::alice_matrix())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Check for `AwaitingSecondChallenge` notification.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::AwaitingSecondChallenge {
            context: alice.context.clone(),
            field: F::alice_email(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Verify second challenge.
    let challenge = VerifyChallenge {
        entry: F::alice_email(),
        challenge: alice
            .get_field(&F::alice_email())
            .expected_second()
            .value
            .clone(),
    };

    let res = api
        .post("/api/verify_second_challenge")
        .send_json(&challenge)
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // Second challenge of email account of Alice is now verified
    alice
        .get_field_mut(&F::alice_email())
        .expected_second_mut()
        .set_verified();

    // Check for `SecondFieldVerified`.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::SecondFieldVerified {
            context: alice.context.clone(),
            field: F::alice_email(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Explicit tests.
    let resp = resp.unwrap();
    // VERIFIED
    assert_eq!(
        resp.state
            .get_field(&F::alice_email())
            .expected_message()
            .is_verified,
        true
    );
    // VERIFIED
    assert_eq!(
        resp.state
            .get_field(&F::alice_email())
            .expected_second()
            .is_verified,
        true
    );
    assert_eq!(
        resp.state
            .get_field(&F::alice_twitter())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::alice_matrix())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Other judgement states must be unaffected (Bob).
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Explicit tests.
    let resp = resp.unwrap();
    assert_eq!(
        resp.state
            .get_field(&F::bob_email())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::bob_twitter())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(
        resp.state
            .get_field(&F::bob_matrix())
            .expected_message()
            .is_verified,
        false
    );
    assert_eq!(resp.state.is_fully_verified, false);
    assert_eq!(resp.state.completion_timestamp, None);
    assert_eq!(resp.state.judgement_submitted, false);

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}