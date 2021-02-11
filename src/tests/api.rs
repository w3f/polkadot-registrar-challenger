use super::{ApiBackend, InMemBackend};
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::event::ErrorMessage;
use crate::manager::IdentityState;
use eventually::Repository;
use futures::StreamExt;
use jsonrpc_core::types::{to_value, Params, Value};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

struct Regulator<T> {
    inner: Arc<RegulatorLock<T>>,
}

struct RegulatorLock<T> {
    lock: Mutex<T>,
    peers: Condvar,
}

struct RegulatorChild<T> {
    my_turn: T,
    inner: Arc<RegulatorLock<T>>,
}

impl<T: Default> Regulator<T> {
    fn new() -> Self {
        Regulator {
            inner: Arc::new(RegulatorLock {
                lock: Mutex::new(T::default()),
                peers: Condvar::new(),
            }),
        }
    }
    fn new_child(&self, my_turn: T) -> RegulatorChild<T> {
        RegulatorChild {
            my_turn: my_turn,
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: PartialEq> RegulatorChild<T> {
    fn yield_time_to(&self, to: T) {
        let mut round = self.inner.lock.lock().unwrap();
        *round = to;
        self.inner.peers.notify_all();
        while *round != self.my_turn {
            // Wait until it's my round again
            round = self.inner.peers.wait(round).unwrap();
        }
    }
}

#[derive(PartialEq)]
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
        let api = ApiBackend::run().await;

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

    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.spawn(async move {
        let api = ApiBackend::run().await;
        let alice = IdentityState::alice();

        #[rustfmt::skip]
        let expected = [
            to_value(ErrorMessage::no_pending_judgement_request(0)).unwrap(),
        ];

        // No pending judgement request is available at first.
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

        // Tell the test to continue with the insertion of the identity.
        reg_stream.yield_time_to(Thread::Aggregate);

        assert_eq!(messages.len(), expected.len());
        /*
        for (message, expected) in messages.iter().zip(expected.iter()) {
            assert_eq!(message, expected)
        }
        */
    });

    reg_aggr.yield_time_to(Thread::Stream);

    let mut rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let es = InMemBackend::<VerifierAggregateId>::run().await;
        let store = es.store();
        let mut repo = Repository::new(VerifierAggregate.into(), store);

        let alice = IdentityState::alice();

        let mut root = repo.get(VerifierAggregateId).await.unwrap();
        root.handle(VerifierCommand::InsertIdentity(alice.clone()))
            .await
            .unwrap();

        reg_aggr.yield_time_to(Thread::Stream);
    });
}

/*
{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus"}

{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus","params":["polkadot","1gfpAmeKYhEoSrEgQ5UDYTiNSeKPvxVfLVWcW73JGnX9L6M"]}
*/
