use super::*;
use crate::actors::api::{JsonResult, ResponseAccountState};
use crate::adapters::admin::{process_admin, Command, RawFieldName, Response};
use crate::primitives::{
    IdentityContext, IdentityField,
    IdentityFieldValue, JudgementState, JudgementStateBlanked, NotificationMessage,
};
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
async fn command_verify_multiple_challenge_types() {
    let (db, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let mut alice = JudgementState::alice();
    db.add_judgement_request(&alice).await.unwrap();

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
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
    let (passed, _) = alice
        .get_field_mut(&F::ALICE_DISPLAY_NAME())
        .expected_display_name_check_mut();
    *passed = true;

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
    let (db, mut api, _) = new_env().await;
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    let mut alice = JudgementState::alice();
    alice
        .fields
        .push(IdentityField::new(IdentityFieldValue::Web(
            "alice.com".to_string(),
        )));

    db.add_judgement_request(&alice).await.unwrap();

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
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
