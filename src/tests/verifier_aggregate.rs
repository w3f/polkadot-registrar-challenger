use super::InMemBackend;
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::event::{Event, EventType, ExternalMessage, ExternalOrigin, FieldStatusVerified};
use crate::manager::{
    ChallengeStatus, FieldAddress, IdentityField, IdentityFieldType, IdentityState, ProvidedMessage, Validity, ExpectedMessage,
};
use eventually::{Repository, Subscription};
use futures::StreamExt;
use hmac::digest::generic_array::typenum::Exp;

#[tokio::test]
async fn insert_identities() {
    let be = InMemBackend::<VerifierAggregateId>::run().await;
    let store = be.store();
    let mut repo = Repository::new(VerifierAggregate.into(), store);

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    let mut root = repo.get(VerifierAggregateId).await.unwrap();

    // Execute commands.
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
    ];

    let events = be.get_events(VerifierAggregateId).await;
    assert_eq!(events.len(), expected.len());

    for (expected, event) in expected.iter().zip(events.iter()) {
        assert_eq!(expected.body, event.body);
    }

    // Check the resulting state.
    let root = repo.get(VerifierAggregateId).await.unwrap();
    let state = root.state();
    assert!(state.contains(&alice));
    assert!(state.contains(&bob));
}

#[tokio::test]
async fn insert_identities_duplicate() {
    let be = InMemBackend::<VerifierAggregateId>::run().await;
    let store = be.store();
    let mut repo = Repository::new(VerifierAggregate.into(), store);

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    let mut root = repo.get(VerifierAggregateId).await.unwrap();

    // Execute commands.
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    // Add duplicate identity
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Add duplicate identity after committed changes.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
    ];

    let events = be.get_events(VerifierAggregateId).await;
    assert_eq!(events.len(), expected.len());

    for (expected, event) in expected.iter().zip(events.iter()) {
        assert_eq!(expected.body, event.body);
    }

    // Check the resulting state.
    let root = repo.get(VerifierAggregateId).await.unwrap();
    let state = root.state();
    assert!(state.contains(&alice));
    assert!(state.contains(&bob));
}

#[tokio::test]
async fn insert_identities_state_change() {
    let be = InMemBackend::<VerifierAggregateId>::run().await;
    let store = be.store();
    let mut repo = Repository::new(VerifierAggregate.into(), store);

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Modify entry of identity.
    let mut alice_new = alice.clone();
    alice_new
        .fields
        .get_mut(&IdentityFieldType::Email)
        .map(|field| {
            *field.mut_field() =
                IdentityField::Email(FieldAddress::from("alice_new@email.com".to_string()))
        })
        .unwrap();

    // Execute commands with new identity state.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    root.handle(VerifierCommand::InsertIdentity(alice_new.clone()))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
        Event::from(EventType::IdentityInserted(alice_new.clone().into())),
    ];

    let events = be.get_events(VerifierAggregateId).await;
    assert_eq!(events.len(), expected.len());

    for (expected, event) in expected.iter().zip(events.iter()) {
        assert_eq!(expected.body, event.body);
    }

    // Check the resulting state.
    let root = repo.get(VerifierAggregateId).await.unwrap();
    let state = root.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));
}

#[tokio::test]
async fn verify_message_valid() {
    let be = InMemBackend::<VerifierAggregateId>::run().await;
    let store = be.store();
    let mut repo = Repository::new(VerifierAggregate.into(), store);

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    let expected_message = alice
        .fields
        .get(&IdentityFieldType::Matrix)
        .map(|field| match field.challenge() {
            ChallengeStatus::ExpectMessage(challenge) => challenge.expected_message.clone(),
            _ => panic!(),
        })
        .unwrap();

    // Execute commands.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Prepare message.
    let message = ExternalMessage {
        origin: ExternalOrigin::Matrix,
        field_address: FieldAddress::from("@alice:matrix.org".to_string()),
        message: ProvidedMessage::from(expected_message),
    };

    // Execute commands.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    root.handle(VerifierCommand::VerifyMessage(message))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Set the expected state.
    let mut alice_new = alice.clone();
    let new_field_state = alice_new.fields.get_mut(&IdentityFieldType::Matrix).map(|status| {
        match status.challenge_mut() {
            ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Valid,
            _ => panic!(),
        }

        status.clone()
    }).unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: new_field_state,
        })),
    ];

    let events = be.get_events(VerifierAggregateId).await;
    assert_eq!(events.len(), expected.len());

    for (expected, event) in expected.iter().zip(events.iter()) {
        assert_eq!(expected.body, event.body);
    }

    // Check the resulting state.
    let root = repo.get(VerifierAggregateId).await.unwrap();
    let state = root.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));
}

#[tokio::test]
async fn verify_message_invalid() {
    let be = InMemBackend::<VerifierAggregateId>::run().await;
    let store = be.store();
    let mut repo = Repository::new(VerifierAggregate.into(), store);

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Prepare message.
    let message = ExternalMessage {
        origin: ExternalOrigin::Matrix,
        field_address: FieldAddress::from("@alice:matrix.org".to_string()),
        message: ProvidedMessage::from(ExpectedMessage::gen()),
    };

    // Execute commands.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    root.handle(VerifierCommand::VerifyMessage(message))
        .await
        .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Set the expected state.
    let mut alice_new = alice.clone();
    let current_state = alice_new.fields.get(&IdentityFieldType::Matrix).unwrap().clone();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: current_state,
        })),
    ];

    let events = be.get_events(VerifierAggregateId).await;
    assert_eq!(events.len(), expected.len());

    for (expected, event) in expected.iter().zip(events.iter()) {
        assert_eq!(expected.body, event.body);
    }

    // Check the resulting state.
    let root = repo.get(VerifierAggregateId).await.unwrap();
    let state = root.state();
    assert!(state.contains(&alice));
    assert!(state.contains(&bob));
}
