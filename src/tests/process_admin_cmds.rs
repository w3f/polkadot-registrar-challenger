use super::*;
use crate::adapters::admin::{process_admin, Command, RawFieldName, Response};
use crate::api::{JsonResult, ResponseAccountState};
use crate::primitives::{
    IdentityContext, IdentityFieldValue, JudgementStateBlanked, NotificationMessage,
};
use futures::{FutureExt, StreamExt};

#[actix::test]
async fn command_status() {
    let (db, connector, _api, _) = new_env().await;

    // Insert judgement request.
    connector.inject(alice_judgement_request()).await;
    let states = connector.inserted_states().await;
    let alice = states[0].clone();

    // Request status.
    let res = process_admin(&db, Command::Status(alice.context.address.clone())).await;
    assert_eq!(res, Response::Status(JudgementStateBlanked::from(alice)));
}

#[actix::test]
async fn command_verify_multiple_challenge_types() {
    let (db, connector, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    connector.inject(alice_judgement_request()).await;
    let states = connector.inserted_states().await;
    let mut alice = states[0].clone();

    // Subscribe to endpoint.
    let resp = subscribe_context(&mut stream, IdentityContext::alice()).await;

    // Check current state.
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Manually verify.
    let resp = process_admin(
        &db,
        Command::Verify(
            alice.context.address.clone(),
            vec![RawFieldName::DisplayName, RawFieldName::Email],
        ),
    )
    .await;

    assert_eq!(
        resp,
        Response::Verified(
            alice.context.address.clone(),
            vec![RawFieldName::DisplayName, RawFieldName::Email]
        )
    );

    // Display name and email are now verified.
    *alice
        .get_field_mut(&F::ALICE_DISPLAY_NAME())
        .expected_display_name_check_mut()
        .0 = true;

    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_message_mut()
        .set_verified();

    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_second_mut()
        .set_verified();

    // Expected display name event
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::ManuallyVerified {
            context: alice.context.clone(),
            field: RawFieldName::DisplayName,
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Expected email event
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::ManuallyVerified {
            context: alice.context.clone(),
            field: RawFieldName::Email,
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Manually verify twitter field.
    let resp = process_admin(
        &db,
        Command::Verify(alice.context.address.clone(), vec![RawFieldName::Twitter]),
    )
    .await;

    assert_eq!(
        resp,
        Response::Verified(alice.context.address.clone(), vec![RawFieldName::Twitter])
    );

    // Twitter and matrix are now verified.
    alice
        .get_field_mut(&F::ALICE_TWITTER())
        .expected_message_mut()
        .set_verified();

    // Expected twitter event
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::ManuallyVerified {
            context: alice.context.clone(),
            field: RawFieldName::Twitter,
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn command_verify_unsupported_field() {
    let (db, connector, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement state with unsupported entry.
    connector
        .inject(WatcherMessage::new_judgement_request({
            let mut req = JudgementRequest::alice();
            req.accounts
                .insert(AccountType::Web, "alice.com".to_string());
            req
        }))
        .await;
    let states = connector.inserted_states().await;
    let mut alice = states[0].clone();

    // Subscribe to endpoint.
    let resp = subscribe_context(&mut stream, IdentityContext::alice()).await;

    // Check current state.
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Manually verify.
    let resp = process_admin(
        &db,
        Command::Verify(alice.context.address.clone(), vec![RawFieldName::Web]),
    )
    .await;

    assert_eq!(
        resp,
        Response::Verified(alice.context.address.clone(), vec![RawFieldName::Web])
    );

    // Web is now verified.
    let is_verified = alice
        .get_field_mut(&IdentityFieldValue::Web("alice.com".to_string()))
        .expected_unsupported_mut();
    *is_verified = Some(true);

    // Expected email event
    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::ManuallyVerified {
            context: alice.context.clone(),
            field: RawFieldName::Web,
        }],
    };

    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn command_verify_all() {
    let (db, connector, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    connector.inject(alice_judgement_request()).await;
    let states = connector.inserted_states().await;
    let mut alice = states[0].clone();

    // Subscribe to endpoint.
    let resp = subscribe_context(&mut stream, IdentityContext::alice()).await;

    // Check current state.
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Manually verify.
    let resp = process_admin(
        &db,
        Command::Verify(alice.context.address.clone(), vec![RawFieldName::All]),
    )
    .await;

    assert_eq!(
        resp,
        Response::FullyVerified(alice.context.address.clone(),)
    );

    // Expected event on stream.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

    // The completion timestamp is not that important, as long as it's `Some`.
    let completion_timestamp = match &resp {
        JsonResult::Ok(r) => r.state.completion_timestamp,
        _ => panic!(),
    };

    assert!(completion_timestamp.is_some());
    alice.is_fully_verified = true;
    alice.judgement_submitted = false;
    alice.completion_timestamp = completion_timestamp;

    // All fields are now verified.
    *alice
        .get_field_mut(&F::ALICE_DISPLAY_NAME())
        .expected_display_name_check_mut()
        .0 = true;

    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_message_mut()
        .set_verified();

    alice
        .get_field_mut(&F::ALICE_EMAIL())
        .expected_second_mut()
        .set_verified();

    alice
        .get_field_mut(&F::ALICE_TWITTER())
        .expected_message_mut()
        .set_verified();

    alice
        .get_field_mut(&F::ALICE_MATRIX())
        .expected_message_mut()
        .set_verified();

    assert!(alice.check_full_verification());

    let expected = ResponseAccountState {
        state: alice.clone().into(),
        notifications: vec![NotificationMessage::FullManualVerification {
            context: alice.context.clone(),
        }],
    };

    assert_eq!(resp, JsonResult::Ok(expected));

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}

#[actix::test]
async fn command_verify_missing_field() {
    let (db, connector, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Remove Email
    let mut request = JudgementRequest::alice();
    request.accounts.remove(&AccountType::Email);

    // Insert judgement request.
    connector
        .inject(WatcherMessage::new_judgement_request(request))
        .await;
    let states = connector.inserted_states().await;
    let alice = states[0].clone();

    // Subscribe to endpoint.
    let resp = subscribe_context(&mut stream, IdentityContext::alice()).await;

    // Check current state.
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
    );

    // Manually verify a field that does not exist.
    let resp = process_admin(
        &db,
        Command::Verify(alice.context.address.clone(), vec![RawFieldName::Email]),
    )
    .await;

    assert_eq!(resp, Response::IdentityNotFound);

    // Empty stream.
    assert!(stream.next().now_or_never().is_none());
}
