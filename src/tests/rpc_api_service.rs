use super::{ApiBackend, ApiClient, InMemBackend};
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::aggregate::Repository;
use crate::api::ConnectionPool;
use crate::event::{ErrorMessage, ExternalMessage, ExternalOrigin, IdentityInserted, StateWrapper};
use crate::manager::{
    ChallengeStatus, FieldAddress, IdentityFieldType, IdentityState, ProvidedMessage,
    UpdateChanges, Validity,
};
use futures::select;
use futures::{future::FusedFuture, Stream, StreamExt, TryStreamExt};
use jsonrpc_core::types::{to_value, Params, Value};
use matrix_sdk::api::r0::account::request_registration_token_via_email;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/*
{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus"}

{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus","params":["polkadot","1gfpAmeKYhEoSrEgQ5UDYTiNSeKPvxVfLVWcW73JGnX9L6M"]}
*/

async fn ensure_empty_stream<S, T>(mut stream: S)
where
    S: Stream<Item = T> + Unpin,
{
    if let Ok(_) = tokio_02::time::timeout(Duration::from_secs(2), stream.next()).await {
        panic!("Stream is not empty");
    }
}

struct Regulator {
    token: Arc<AtomicBool>,
}

impl Regulator {
    fn new() -> (TokenV1, TokenV02) {
        let token = Arc::new(AtomicBool::new(true));

        (
            TokenV1 {
                shared: Arc::clone(&token),
                my_token: true,
            },
            TokenV02 {
                shared: Arc::clone(&token),
                my_token: false,
            },
        )
    }
}

/// Token type for tokio runtime version v1.
struct TokenV1 {
    shared: Arc<AtomicBool>,
    my_token: bool,
}

impl TokenV1 {
    fn me_first(&self) {
        self.shared.store(self.my_token, Ordering::Relaxed);
    }
    // Rotate turn: let the other thread continue, and wait until the turn has
    // been rotated back.
    async fn rotate(&self) {
        self.shared.store(!self.my_token, Ordering::Relaxed);
        while self.shared.load(Ordering::Relaxed) != self.my_token {
            tokio::time::sleep(Duration::from_millis(50)).await
        }
    }
    // Wait until its the token holder's turn.
    async fn wait(&self) {
        while self.shared.load(Ordering::Relaxed) != self.my_token {
            tokio::time::sleep(Duration::from_millis(50)).await
        }
    }
    async fn wait_forever(&self) {
        loop {
            tokio::time::sleep(Duration::from_millis(1_000)).await
        }
    }
}

/// Token type for tokio runtime version v0.2.
struct TokenV02 {
    shared: Arc<AtomicBool>,
    my_token: bool,
}

impl TokenV02 {
    fn me_first(&self) {
        self.shared.store(self.my_token, Ordering::Relaxed);
    }
    // Rotate turn: let the other thread continue, and wait until the turn has
    // been rotated back.
    async fn rotate(&self) {
        self.shared.store(!self.my_token, Ordering::Relaxed);
        while self.shared.load(Ordering::Relaxed) != self.my_token {
            tokio_02::time::delay_for(Duration::from_millis(50)).await
        }
    }
    async fn wait(&self) {
        while self.shared.load(Ordering::Relaxed) != self.my_token {
            tokio_02::time::delay_for(Duration::from_millis(50)).await
        }
    }
}

#[test]
fn subscribe_status_no_judgement_request() {
    let shared_port = Arc::new(AtomicUsize::new(0));
    let (tokenv1, tokenv2) = Regulator::new();

    tokenv1.me_first();

    // Run the API service (tokio v1).
    let t_shared_port = Arc::clone(&shared_port);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.spawn(async move {
        let be = InMemBackend::run().await;
        let store = be.store();
        let port = ApiBackend::run(store).await;
        t_shared_port.store(port, Ordering::Relaxed);

        // Let the server spin up.
        tokio::time::sleep(Duration::from_secs(2)).await;
        tokenv1.rotate().await;
    });

    // Make tests with the client (tokio v0.2).
    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        tokenv2.wait().await;

        let client = ApiClient::new(shared_port.load(Ordering::Relaxed)).await;
        let alice = IdentityState::alice();

        #[rustfmt::skip]
        let expected = [
            to_value(ErrorMessage::no_pending_judgement_request(0)).unwrap(),
        ];

        // No pending judgement request is available.
        let messages = client
            .get_messages(
                "account_subscribeStatus",
                Params::Array(vec![
                    Value::String(alice.net_address.net_str().to_string()),
                    Value::String(alice.net_address.address_str().to_string()),
                ]),
                "account_status",
                "account_unsubscribeStatus",
            )
            .await;

        assert_eq!(messages.len(), expected.len());
        for (message, expected) in messages.iter().zip(expected.iter()) {
            assert_eq!(message, expected)
        }
    });
}

#[test]
fn subscribe_status_newly_inserted_identity_in_steps() {
    let shared_port = Arc::new(AtomicUsize::new(0));
    let (tokenv1, tokenv2) = Regulator::new();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();
    let t_alice = alice.clone();

    tokenv1.me_first();

    // Run the API service (tokio v1).
    let t_shared_port = Arc::clone(&shared_port);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.spawn(async move {
        let be = InMemBackend::run().await;
        let store = be.store();
        let port = ApiBackend::run(store.clone()).await;
        t_shared_port.store(port, Ordering::Relaxed);

        // Let the server spin up.
        tokio::time::sleep(Duration::from_secs(2)).await;
        tokenv1.rotate().await;

        let aggregate = VerifierAggregate::default().set_snapshot_every(1);
        let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
            .await
            .unwrap();

        // Execute commands.
        repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
            .await
            .unwrap();

        repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
            .await
            .unwrap();

        tokenv1.rotate().await;
        tokenv1.wait_forever().await;
    });

    // Make tests with the client (tokio v0.2).
    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        tokenv2.wait().await;

        let client = ApiClient::new(shared_port.load(Ordering::Relaxed)).await;
        let alice = t_alice;

        #[rustfmt::skip]
        let expected = [
            to_value(ErrorMessage::no_pending_judgement_request(0)).unwrap(),
            to_value(StateWrapper::newly_inserted_notification(IdentityInserted { identity: alice.clone()} )).unwrap(),
        ];

        let mut stream = client
            .raw()
            .subscribe(
                "account_subscribeStatus",
                Params::Array(vec![
                    Value::String(alice.net_address.net_str().to_string()),
                    Value::String(alice.net_address.address_str().to_string()),
                ]),
                "account_status",
                "account_unsubscribeStatus",
            )
            .unwrap();

        assert_eq!(stream.next().await.unwrap().unwrap(), expected[0]);
        tokenv2.rotate().await;

        // Check result on newly  inserted identity.
        assert_eq!(stream.next().await.unwrap().unwrap(), expected[1]);

        // Client only subscribed to Alice, so no notification about Bob.
        ensure_empty_stream(stream).await;
    });
}

#[test]
fn subscribe_status_verified_changes_in_steps() {
    let shared_port = Arc::new(AtomicUsize::new(0));
    let (tokenv1, tokenv2) = Regulator::new();

    let alice = IdentityState::alice();
    let bob = IdentityState::bob();
    let t_alice = alice.clone();

    tokenv1.me_first();

    // Run the API service (tokio v1).
    let t_shared_port = Arc::clone(&shared_port);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.spawn(async move {
        let be = InMemBackend::run().await;
        let store = be.store();
        let port = ApiBackend::run(store.clone()).await;
        t_shared_port.store(port, Ordering::Relaxed);

        // Let the server spin up.
        tokio::time::sleep(Duration::from_secs(2)).await;
        tokenv1.rotate().await;

        let aggregate = VerifierAggregate::default().set_snapshot_every(1);
        let mut repo = Repository::new_with_snapshot_service(aggregate, store.clone())
            .await
            .unwrap();

        // Execute commands.
        repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
            .await
            .unwrap();

        repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
            .await
            .unwrap();

        tokenv1.rotate().await;

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

        tokenv1.rotate().await;
        tokenv1.wait_forever().await;
    });

    // Make tests with the client (tokio v0.2).
    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        tokenv2.wait().await;

        let client = ApiClient::new(shared_port.load(Ordering::Relaxed)).await;
        let alice = t_alice;

        // Get the expected field.
        let field = alice
            .fields
            .get(&IdentityFieldType::Matrix)
            .map(|field| {
                field.field.clone()
            })
            .unwrap();

        // Set the expected state.
        let mut alice_new = alice.clone();
        alice_new
            .fields
            .get_mut(&IdentityFieldType::Matrix)
            .map(|status| {
                match status.challenge_mut() {
                    ChallengeStatus::ExpectMessage(challenge) => challenge.status = Validity::Valid,
                    _ => panic!(),
                }
            })
            .unwrap();


        #[rustfmt::skip]
        let expected = [
            to_value(ErrorMessage::no_pending_judgement_request(0)).unwrap(),
            to_value(StateWrapper::newly_inserted_notification(IdentityInserted { identity: alice.clone()} )).unwrap(),
            to_value(StateWrapper::with_notifications(alice_new.clone(), vec![UpdateChanges::VerificationValid(field).into()])).unwrap(),
        ];

        let mut stream = client
            .raw()
            .subscribe(
                "account_subscribeStatus",
                Params::Array(vec![
                    Value::String(alice.net_address.net_str().to_string()),
                    Value::String(alice.net_address.address_str().to_string()),
                ]),
                "account_status",
                "account_unsubscribeStatus",
            )
            .unwrap();

        assert_eq!(stream.next().await.unwrap().unwrap(), expected[0]);
        tokenv2.rotate().await;

        assert_eq!(stream.next().await.unwrap().unwrap(), expected[1]);
        tokenv2.rotate().await;

        assert_eq!(stream.next().await.unwrap().unwrap(), expected[2]);

        // Client only subscribed to Alice, so no notification about Bob.
        ensure_empty_stream(stream).await;
    });
}
