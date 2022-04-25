use super::*;
use crate::actors::api::VerifyChallenge;
use crate::actors::api::{JsonResult, ResponseAccountState};
use crate::actors::connector::tests::{ConnectorMocker, OutgoingCounter};
use crate::actors::connector::WatcherMessage;
use crate::primitives::{
    ExpectedMessage, ExternalMessage, ExternalMessageType, IdentityContext, JudgementState,
    MessageId, NotificationMessage, Timestamp,
};
use actix_http::StatusCode;
use futures::{FutureExt, SinkExt, StreamExt};

#[actix::test]
async fn current_judgement_state_single_identity() {
    let (db, connector, mut api, _inj) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    connector.inject(alice_judgement_request()).await;

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
    let (db, connector, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    // Multiple inserts of the same request. Must not cause bad behavior.
    connector.inject(alice_judgement_request()).await;
    connector.inject(alice_judgement_request()).await;

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
    let (db, connector, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    let bob = JudgementState::bob();
    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state (Alice).
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );

    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    // Check current state (Bob).
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
    let (db, connector, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let bob = JudgementState::bob();
    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;

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
    *alice.get_field_mut(&F::ALICE_EMAIL()).failed_attempts_mut() = 1;

    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerificationFailed {
            context: alice.context.clone(),
            field: F::ALICE_EMAIL(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Other judgement states must be unaffected (Bob).
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
    let (db, connector, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let alice = JudgementState::alice();
    let bob = JudgementState::bob();
    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;

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
                .get_field(&F::ALICE_EMAIL())
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

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_valid_message() {
    let (db, connector, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let bob = JudgementState::bob();
    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;

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
                .get_field(&F::ALICE_MATRIX())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Matrix account of Alice is now verified
    alice
        .get_field_mut(&F::ALICE_MATRIX())
        .expected_message_mut()
        .set_verified();

    // The expected message (field verified successfully).
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: F::ALICE_MATRIX(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Other judgement states must be unaffected (Bob).
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
async fn verify_valid_message_duplicate_account_name() {
    let (db, connector, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let mut bob = JudgementState::bob();

    // Bob also has the same Matrix account as Alice.
    bob.get_field_mut(&F::BOB_MATRIX()).value = F::ALICE_MATRIX();

    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;

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
                .get_field(&F::ALICE_MATRIX())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Email account of Alice is now verified
    alice
        .get_field_mut(&F::ALICE_MATRIX())
        .expected_message_mut()
        .set_verified();

    // The expected message (field verified successfully).
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: F::ALICE_MATRIX(),
        }],
    };

    // Check response
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Other judgement states must be unaffected (Bob), but will receive a "failed attempt".
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    *bob.get_field_mut(&F::ALICE_MATRIX()).failed_attempts_mut() = 1;

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn verify_valid_message_awaiting_second_challenge() {
    let (db, connector, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let bob = JudgementState::bob();

    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;

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
                .get_field(&F::ALICE_EMAIL())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Email account of Alice is now verified, but not the second challenge.
    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_message_mut()
        .set_verified();

    // Check for `FieldVerified` notification.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: F::ALICE_EMAIL(),
        }],
    };

    // Check response.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Check for `AwaitingSecondChallenge` notification.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::AwaitingSecondChallenge {
            context: alice.context.clone(),
            field: F::ALICE_EMAIL(),
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Verify second challenge.
    let challenge = VerifyChallenge {
        entry: F::ALICE_EMAIL(),
        challenge: alice
            .get_field(&F::ALICE_EMAIL())
            .expected_second()
            .value
            .clone(),
    };

    // Send it to the API endpoint.
    let res = api
        .post("/api/verify_second_challenge")
        .send_json(&challenge)
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // Second challenge of email account of Alice is now verified
    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_second_mut()
        .set_verified();

    // Check for `SecondFieldVerified` notification.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::SecondFieldVerified {
            context: alice.context.clone(),
            field: F::ALICE_EMAIL(),
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Other judgement state must be unaffected (Bob).
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
async fn verify_invalid_message_awaiting_second_challenge() {
    let (db, connector, mut api, injector) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement requests.
    let mut alice = JudgementState::alice();
    let bob = JudgementState::bob();

    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;

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
                .get_field(&F::ALICE_EMAIL())
                .expected_message()
                .to_message_parts(),
        })
        .await;

    // Email account of Alice is now verified, but not the second challenge.
    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_message_mut()
        .set_verified();

    // Check for `FieldVerified` notification.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FieldVerified {
            context: alice.context.clone(),
            field: F::ALICE_EMAIL(),
        }],
    };

    // Check response.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Check for `AwaitingSecondChallenge` notification.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::AwaitingSecondChallenge {
            context: alice.context.clone(),
            field: F::ALICE_EMAIL(),
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Send invalid second challenge.
    let challenge = VerifyChallenge {
        entry: F::ALICE_EMAIL(),
        challenge: "INVALID".to_string(),
    };

    // Send it to the API endpoint.
    let res = api
        .post("/api/verify_second_challenge")
        .send_json(&challenge)
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    // Check for `SecondFieldVerified` notification.
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::SecondFieldVerificationFailed {
            context: alice.context.clone(),
            field: F::ALICE_EMAIL(),
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Other judgement state must be unaffected (Bob).
    stream.send(IdentityContext::bob().to_ws()).await.unwrap();

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
    );

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}
