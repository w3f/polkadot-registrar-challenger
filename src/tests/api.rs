use super::{ApiBackend, InMemBackend};
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::api::ConnectionPool;
use crate::event::ErrorMessage;
use crate::manager::IdentityState;
use futures::StreamExt;
use jsonrpc_core::types::{to_value, Params, Value};
use matrix_sdk::api::r0::account::request_registration_token_via_email;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/*
struct Regulator<T> {
    inner: Arc<RegulatorLock<T>>,
}

struct RegulatorLock<T> {
    lock: Mutex<Option<T>>,
    peers: Condvar,
}

struct RegulatorChild<T> {
    my_turn: Option<T>,
    inner: Arc<RegulatorLock<T>>,
}

impl<T> Regulator<T> {
    fn new() -> Self {
        Regulator {
            inner: Arc::new(RegulatorLock {
                lock: Mutex::new(None),
                peers: Condvar::new(),
            }),
        }
    }
    fn new_child(&self, my_turn: T) -> RegulatorChild<T> {
        RegulatorChild {
            my_turn: Some(my_turn),
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: PartialEq + std::fmt::Debug> RegulatorChild<T> {
    fn yield_time_to(&self, to: T) {
        self.yield_and_exit(to);
        self.wait()
    }
    fn yield_and_exit(&self, to: T) {
        let mut round = self.inner.lock.lock().unwrap();
        *round = Some(to);

        drop(round);

        self.inner.peers.notify_all();
    }
    fn wait(&self) {
        let mut round = self.inner.lock.lock().unwrap();
        while *round != self.my_turn {
            // Wait until it's my round again
            round = self.inner.peers.wait(round).unwrap();
        }
    }
}

#[derive(Debug, PartialEq)]
enum Thread {
    Aggregate,
    Stream,
}

impl Default for Thread {
    fn default() -> Self {
        Thread::Aggregate
    }
}

#[test]
fn subscribe_status_no_judgement_request() {
    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let api = ApiBackend::run(ConnectionPool::default()).await;

        let alice = IdentityState::alice();

        #[rustfmt::skip]
        let expected = [
            to_value(ErrorMessage::no_pending_judgement_request(0)).unwrap(),
        ];

        // No pending judgement request is available.
        let messages = api
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
