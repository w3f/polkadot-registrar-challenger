use super::InMemBackend;
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::event::{Event, EventType};
use crate::manager::IdentityState;
use eventually::{Repository, Subscription};
use futures::StreamExt;

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
async fn insert_identities_repeated() {
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
    let mut root = repo.get(VerifierAggregateId).await.unwrap();

    // Add duplicate identity after state change.
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
