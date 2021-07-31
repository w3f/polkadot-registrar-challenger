use super::*;
use crate::DisplayNameConfig;
use crate::actors::api::{JsonResult, ResponseAccountState};
use crate::actors::connector::DisplayNameEntry;
use crate::display_name::DisplayNameVerifier;
use crate::primitives::{ChainAddress, ExpectedMessage, ExternalMessage, ExternalMessageType, IdentityContext, IdentityFieldValue, JudgementState, MessageId, NotificationMessage, Timestamp};
use futures::{FutureExt, SinkExt, StreamExt};

impl From<&str> for DisplayNameEntry {
	fn from(val: &str) -> Self {
		DisplayNameEntry {
			display_name: val.to_string(),
			address: ChainAddress::from(""),
		}
	}
}

fn config() -> DisplayNameConfig {
	DisplayNameConfig {
		enabled: true,
		limit: 0.85,
	}
}

#[actix::test]
async fn valid_display_name() {
    let (db, mut api, _) = new_env().await;
	let verifier = DisplayNameVerifier::new(db.clone(), config());

    // Insert judgement request.
    let alice = JudgementState::alice();
    db.add_judgement_request(&alice).await.unwrap();
	verifier.verify_display_name(&alice).await.unwrap();

    // Subscribe to endpoint.
    let mut stream = api.ws_at("/api/account_status").await.unwrap();
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

    // Check current state.
    assert_eq!(
        resp,
        JsonResult::Ok(ResponseAccountState::with_no_notifications(alice))
    );
}

#[actix::test]
async fn invalid_display_name() {
    let (db, mut api, _) = new_env().await;
	let verifier = DisplayNameVerifier::new(db.clone(), config());

	// Pre-fill database with active display names
	let names = vec![
		DisplayNameEntry::from("Alice"),
		DisplayNameEntry::from("alice"),
		DisplayNameEntry::from("Alicee"),
	];

	for name in &names {
		db.insert_display_name(name).await.unwrap();
	}

    // Insert judgement request.
    let mut alice = JudgementState::alice();
    db.add_judgement_request(&alice).await.unwrap();
	verifier.verify_display_name(&alice).await.unwrap();

    // Subscribe to endpoint.
    let mut stream = api.ws_at("/api/account_status").await.unwrap();
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();

	// Set expected result.
	let mut field = alice.get_field_mut(&IdentityFieldValue::DisplayName("Alice".to_string()));
	let (passed, violations) = field.expected_display_name_check_mut();
	*passed = false;
	*violations = names;

	let expected = ResponseAccountState {
		state: alice.into(),
		// TODO: Should probably have some.
		notifications: vec![],
	};

    // Check expected state.
    assert_eq!(
        resp,
		JsonResult::Ok(expected),
    );
}

