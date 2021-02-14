use super::{ApiBackend, ApiClient, InMemBackend};
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::api::ConnectionPool;
use crate::event::ErrorMessage;
use crate::manager::IdentityState;
use futures::StreamExt;
use jsonrpc_core::types::{to_value, Params, Value};
use matrix_sdk::api::r0::account::request_registration_token_via_email;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use std::sync::mpsc::channel;

struct Regulator {
    token: Arc<AtomicBool>,
}

impl Regulator {
    fn new() -> (TokenV1, TokenV02) {
        let token = Arc::new(AtomicBool::new(true));

        (
            TokenV1 {
                shared: Arc::clone(&token),
                my_token: true
            },
            TokenV02 {
                shared: Arc::clone(&token),
                my_token: false 
            }
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

/*
#[test]
fn subscribe_status_pending_judgement_request() {
    let regulator = Regulator::new();
    let reg_aggr = regulator.new_child(Thread::Aggregate);
    let reg_stream = regulator.new_child(Thread::Stream);

    let pool = ConnectionPool::default();
    let t_pool = pool.clone();

    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.spawn(async move {
        let api = ApiBackend::run(t_pool).await;
        let alice = IdentityState::alice();

        reg_stream.wait();

        #[rustfmt::skip]
        let expected = [
            to_value(ErrorMessage::no_pending_judgement_request(0)).unwrap(),
        ];

        // No pending judgement request is available at first.
        let mut stream = api
            .client()
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

        let mut counter = 0;
        while let Some(message) = stream.next().await {
            //assert_eq!(message.unwrap(), expected[counter]);
            println!(">>> {:?}", message.unwrap());

            reg_stream.yield_and_exit(Thread::Aggregate);

            counter += 1;
        }
    });

    reg_aggr.yield_time_to(Thread::Stream);

    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let es = InMemBackend::<VerifierAggregateId>::run().await;
        es.run_session_notifier(pool).await;
        let store = es.store();
        let mut repo = Repository::new(VerifierAggregate.into(), store);

        let alice = IdentityState::alice();

        let mut root = repo.get(VerifierAggregateId).await.unwrap();
        root.handle(VerifierCommand::InsertIdentity(alice.clone()))
            .await
            .unwrap();

        // Commit changes
        repo.add(root).await.unwrap();

        reg_aggr.yield_time_to(Thread::Stream);
    });
}
*/

/*
{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus"}

{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus","params":["polkadot","1gfpAmeKYhEoSrEgQ5UDYTiNSeKPvxVfLVWcW73JGnX9L6M"]}
*/
