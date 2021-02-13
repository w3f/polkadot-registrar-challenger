use super::InMemBackend;
use crate::aggregate::Repository;
use crate::aggregate::{
    verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand},
    Snapshot,
};
use crate::event::{
    DisplayNamePersisted, Event, EventType, ExternalMessage, ExternalOrigin, FieldStatusVerified,
};
use crate::manager::{
    ChallengeStatus, DisplayName, ExpectedMessage, FieldAddress, FieldStatus, IdentityField,
    IdentityFieldType, IdentityState, ProvidedMessage, RegistrarIdentityField, Validity,
};
use futures::StreamExt;
use std::convert::TryFrom;

#[tokio::test]
async fn insert_identities() {
    let be = InMemBackend::run().await;
    let store = be.store();
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
        .await
        .unwrap();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

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
    let state = repo.state();
    assert!(state.contains(&alice));
    assert!(state.contains(&bob));

    // Check snapshot.
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let repo = Repository::new_with_snapshot_service(aggregate, store)
        .await
        .unwrap();

    let state = repo.state();
    assert!(state.contains(&alice));
    assert!(state.contains(&bob));
}

#[tokio::test]
async fn insert_identities_duplicate() {
    let be = InMemBackend::run().await;
    let store = be.store();
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
        .await
        .unwrap();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    // Add duplicate identity
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

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
    let state = repo.state();
    assert!(state.contains(&alice));
    assert!(state.contains(&bob));

    // Check snapshot.
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let repo = Repository::new_with_snapshot_service(aggregate, store)
        .await
        .unwrap();

    let state = repo.state();
    assert!(state.contains(&alice));
    assert!(state.contains(&bob));
}

#[tokio::test]
async fn insert_identities_state_change() {
    let be = InMemBackend::run().await;
    let store = be.store();
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
        .await
        .unwrap();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

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
    repo.apply(VerifierCommand::InsertIdentity(alice_new.clone()))
        .await
        .unwrap();

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
    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));

    // Check snapshot.
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let repo = Repository::new_with_snapshot_service(aggregate, store)
        .await
        .unwrap();

    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));
}

#[tokio::test]
async fn verify_message_valid_message() {
    let be = InMemBackend::run().await;
    let store = be.store();
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
        .await
        .unwrap();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Prepare message.
    let expected_message = alice
        .fields
        .get(&IdentityFieldType::Matrix)
        .map(|field| match field.challenge() {
            ChallengeStatus::ExpectMessage(challenge) => challenge.expected_message.clone(),
            _ => panic!(),
        })
        .unwrap();

    let message = ExternalMessage {
        origin: ExternalOrigin::Matrix,
        field_address: FieldAddress::from("@alice:matrix.org".to_string()),
        message: ProvidedMessage::from(expected_message),
    };

    // Execute commands.
    repo.apply(VerifierCommand::VerifyMessage(message))
        .await
        .unwrap();

    // Set the expected state.
    let mut alice_new = alice.clone();
    let new_field_state = alice_new
        .fields
        .get_mut(&IdentityFieldType::Matrix)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Valid,
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

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
    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));

    // Check snapshot.
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let repo = Repository::new_with_snapshot_service(aggregate, store)
        .await
        .unwrap();

    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));
}

#[tokio::test]
async fn verify_message_invalid_message() {
    let be = InMemBackend::run().await;
    let store = be.store();
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
        .await
        .unwrap();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Prepare message.
    let message = ExternalMessage {
        origin: ExternalOrigin::Matrix,
        field_address: FieldAddress::from("@alice:matrix.org".to_string()),
        message: ProvidedMessage::from(ExpectedMessage::gen()),
    };

    // Execute commands.
    repo.apply(VerifierCommand::VerifyMessage(message))
        .await
        .unwrap();

    // Set the expected state.
    let mut alice_new = alice.clone();
    let alice_invalid_state = alice_new
        .fields
        .get_mut(&IdentityFieldType::Matrix)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Invalid,
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: alice_invalid_state,
        })),
    ];

    let events = be.get_events(VerifierAggregateId).await;
    assert_eq!(events.len(), expected.len());

    for (expected, event) in expected.iter().zip(events.iter()) {
        assert_eq!(expected.body, event.body);
    }

    // Check the resulting state.
    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));

    // Check snapshot
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let repo = Repository::new_with_snapshot_service(aggregate, store)
        .await
        .unwrap();

    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));
}

#[tokio::test]
async fn verify_message_valid_message_multiple() {
    let be = InMemBackend::run().await;
    let store = be.store();
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
        .await
        .unwrap();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Execute commands.
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Prepare message.
    let expected_message = alice
        .fields
        .get(&IdentityFieldType::Matrix)
        .map(|field| match field.challenge() {
            ChallengeStatus::ExpectMessage(challenge) => challenge.expected_message.clone(),
            _ => panic!(),
        })
        .unwrap();

    let messages = vec![
        // Invalid
        ExternalMessage {
            origin: ExternalOrigin::Matrix,
            field_address: FieldAddress::from("@alice:matrix.org".to_string()),
            message: ProvidedMessage::from(ExpectedMessage::gen()),
        },
        // Invalid
        ExternalMessage {
            origin: ExternalOrigin::Matrix,
            field_address: FieldAddress::from("@alice:matrix.org".to_string()),
            message: ProvidedMessage::from(ExpectedMessage::gen()),
        },
        // Valid
        ExternalMessage {
            origin: ExternalOrigin::Matrix,
            field_address: FieldAddress::from("@alice:matrix.org".to_string()),
            message: ProvidedMessage::from(expected_message.clone()),
        },
        // Valid second time
        ExternalMessage {
            origin: ExternalOrigin::Matrix,
            field_address: FieldAddress::from("@alice:matrix.org".to_string()),
            message: ProvidedMessage::from(expected_message),
        },
        // Invalid
        ExternalMessage {
            origin: ExternalOrigin::Matrix,
            field_address: FieldAddress::from("@alice:matrix.org".to_string()),
            message: ProvidedMessage::from(ExpectedMessage::gen()),
        },
    ];

    // Execute commands.
    for message in messages {
        repo.apply(VerifierCommand::VerifyMessage(message))
            .await
            .unwrap();
    }

    // Specify the expected states.
    let mut alice_new = alice.clone();
    let alice_invalid_state = alice_new
        .fields
        .get_mut(&IdentityFieldType::Matrix)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Invalid,
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

    let alice_valid_state = alice_new
        .fields
        .get_mut(&IdentityFieldType::Matrix)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Valid,
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: alice_invalid_state.clone(),
        })),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: alice_invalid_state,
        })),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: alice_valid_state,
        })),
        // Since the field has been verified, no new event is created, even
        // though a invalid message has been sent after verification.
    ];

    let events = be.get_events(VerifierAggregateId).await;
    assert_eq!(events.len(), expected.len());

    for (expected, event) in expected.iter().zip(events.iter()) {
        assert_eq!(expected.body, event.body);
    }

    // Check the resulting state.
    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));

    // Check snapshot.
    let aggregate = VerifierAggregate::default().set_snapshot_every(1);
    let repo = Repository::new_with_snapshot_service(aggregate, store)
        .await
        .unwrap();

    let state = repo.state();
    assert!(!state.contains(&alice));
    assert!(state.contains(&bob));
    assert!(state.contains(&alice_new));
}

/*
#[tokio::test]
async fn verify_message_invalid_origin() {
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
    let expected_message = alice
        .fields
        .get(&IdentityFieldType::Matrix)
        .map(|field| match field.challenge() {
            ChallengeStatus::ExpectMessage(challenge) => challenge.expected_message.clone(),
            _ => panic!(),
        })
        .unwrap();

    // Invalid messages, even though the message itself is valid:
    // * 1. The sender is invalid.
    // * 2. The sender does not exist at all.
    // * 3. The origin is wrong.
    // * 4. The origin is wrong, even though the address itself matches the email.
    let messages = vec![
        ExternalMessage {
            origin: ExternalOrigin::Matrix,
            field_address: FieldAddress::from("@bob:matrix.org".to_string()),
            message: ProvidedMessage::from(expected_message.clone()),
        },
        ExternalMessage {
            origin: ExternalOrigin::Matrix,
            field_address: FieldAddress::from("@eve:matrix.org".to_string()),
            message: ProvidedMessage::from(expected_message.clone()),
        },
        ExternalMessage {
            origin: ExternalOrigin::Twitter,
            field_address: FieldAddress::from("@alice".to_string()),
            message: ProvidedMessage::from(expected_message.clone()),
        },
        ExternalMessage {
            origin: ExternalOrigin::Twitter,
            field_address: FieldAddress::from("@alice:matrix.org".to_string()),
            message: ProvidedMessage::from(expected_message),
        },
    ];

    // Execute commands.
    let mut root = repo.get(VerifierAggregateId).await.unwrap();
    for message in messages {
        root.handle(VerifierCommand::VerifyMessage(message))
            .await
            .unwrap();
    }

    // Commit changes
    repo.add(root).await.unwrap();

    // Specify the expected states.
    let mut alice_new = alice.clone();
    let alice_invalid_state = alice_new
        .fields
        .get_mut(&IdentityFieldType::Twitter)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Invalid,
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

    let mut bob_new = bob.clone();
    let bob_invalid_state = bob_new
        .fields
        .get_mut(&IdentityFieldType::Matrix)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Invalid,
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
        // Even though multiple messages have been sent, only two events are
        // generated, since the service ignores messages from unknown senders
        // (and matching their corresponding origin).
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: bob.net_address.clone(),
            field_status: bob_invalid_state,
        })),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: alice_invalid_state,
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
    assert!(!state.contains(&bob));
    assert!(state.contains(&alice_new));
    assert!(state.contains(&bob_new));
}

#[tokio::test]
async fn verify_and_persist_display_names() {
    let be = InMemBackend::<VerifierAggregateId>::run().await;
    let store = be.store();
    let mut repo = Repository::new(VerifierAggregate.into(), store);

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();
    let mut eve = IdentityState::eve();

    let alice_display_name = DisplayName::from("Alice");
    let bob_display_name = DisplayName::from("Bob");

    // Eve takes Alices's display name:
    eve.fields.insert(IdentityFieldType::DisplayName, {
        FieldStatus::try_from({
            (
                IdentityField::DisplayName(DisplayName::from("Alice")),
                RegistrarIdentityField::display_name(),
            )
        })
        .unwrap()
    });

    let mut root = repo.get(VerifierAggregateId).await.unwrap();

    // Execute commands.
    root.handle(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::InsertIdentity(eve.clone()))
        .await
        .unwrap();

    root.handle(VerifierCommand::PersistDisplayName {
        net_address: eve.net_address.clone(),
        display_name: alice_display_name.clone(),
    })
    .await
    .unwrap();

    root.handle(VerifierCommand::VerifyDisplayName {
        net_address: alice.net_address.clone(),
        display_name: alice_display_name.clone(),
    })
    .await
    .unwrap();

    root.handle(VerifierCommand::VerifyDisplayName {
        net_address: bob.net_address.clone(),
        display_name: bob_display_name,
    })
    .await
    .unwrap();

    // Commit changes
    repo.add(root).await.unwrap();

    // Set the expected state.
    let mut alice_new = alice.clone();
    let alice_invalid_state = alice_new
        .fields
        .get_mut(&IdentityFieldType::DisplayName)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::CheckDisplayName(challenge) => {
                    challenge.status = Validity::Invalid;
                    challenge.similarities = Some(vec![alice_display_name.clone()])
                }
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

    let mut bob_new = bob.clone();
    let bob_valid_state = bob_new
        .fields
        .get_mut(&IdentityFieldType::DisplayName)
        .map(|status| {
            match status.challenge_mut() {
                ChallengeStatus::CheckDisplayName(challenge) => {
                    challenge.status = Validity::Valid;
                    challenge.similarities = None;
                }
                _ => panic!(),
            }

            status.clone()
        })
        .unwrap();

    // Check the resulting events.
    let expected = [
        Event::from(EventType::IdentityInserted(alice.clone().into())),
        Event::from(EventType::IdentityInserted(bob.clone().into())),
        Event::from(EventType::IdentityInserted(eve.clone().into())),
        Event::from(EventType::DisplayNamePersisted(DisplayNamePersisted {
            net_address: eve.net_address.clone(),
            display_name: alice_display_name,
        })),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: alice.net_address.clone(),
            field_status: alice_invalid_state,
        })),
        Event::from(EventType::FieldStatusVerified(FieldStatusVerified {
            net_address: bob.net_address.clone(),
            field_status: bob_valid_state,
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
    assert!(!state.contains(&bob));
    assert!(state.contains(&alice_new));
    assert!(state.contains(&bob_new));
    assert!(state.contains(&eve));
}
*/
