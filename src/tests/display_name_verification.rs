use super::*;
use crate::api::{JsonResult, ResponseAccountState};
use crate::connector::DisplayNameEntry;
use crate::display_name::DisplayNameVerifier;
use crate::primitives::{IdentityContext, IdentityFieldValue};
use crate::DisplayNameConfig;
use futures::{SinkExt, StreamExt};

impl From<&str> for DisplayNameEntry {
    fn from(val: &str) -> Self {
        DisplayNameEntry {
            display_name: val.to_string(),
            // Filler value.
            context: IdentityContext::bob(),
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
    let (db, connector, mut api, _) = new_env().await;
    let verifier = DisplayNameVerifier::new(db.clone(), config());
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

    // Insert judgement request.
    connector.inject(alice_judgement_request()).await;
    let states = connector.inserted_states().await;
    let mut alice = states[0].clone();
    verifier.verify_display_name(&alice).await.unwrap();

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Set expected result.
    let field = alice.get_field_mut(&IdentityFieldValue::DisplayName("Alice".to_string()));
    let (passed, violations) = field.expected_display_name_check_mut();
    *passed = true;
    *violations = vec![];

    let expected = ResponseAccountState {
        state: alice.into(),
        // The UI already shows invalid display names in a specific way,
        // notification is not required.
        notifications: vec![],
    };

    // Check current state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));
}

#[actix::test]
async fn invalid_display_name() {
    let (db, connector, mut api, _) = new_env().await;
    let verifier = DisplayNameVerifier::new(db.clone(), config());
    let mut stream = api.ws_at("/api/account_status").await.unwrap();

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
    connector.inject(alice_judgement_request()).await;
    let states = connector.inserted_states().await;
    let mut alice = states[0].clone();
    verifier.verify_display_name(&alice).await.unwrap();

    // Subscribe to endpoint.
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();

    // Set expected result.
    let field = alice.get_field_mut(&IdentityFieldValue::DisplayName("Alice".to_string()));
    let (passed, violations) = field.expected_display_name_check_mut();
    *passed = false;
    *violations = names;

    let expected = ResponseAccountState {
        state: alice.into(),
        // The UI already shows invalid display names in a specific way,
        // notification is not required.
        notifications: vec![],
    };

    // Check expected state.
    let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
    assert_eq!(resp, JsonResult::Ok(expected));
}
