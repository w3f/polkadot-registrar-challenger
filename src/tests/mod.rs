use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId};
use crate::manager::{
    FieldAddress, FieldStatus, IdentityAddress, IdentityState, NetworkAddress,
    RegistrarIdentityField,
};
use crate::system::run_api_service;
use eventually_event_store_db::{EventStore, EventStoreBuilder};
use jsonrpc_ws_server::Server as WsServer;
use rand::{thread_rng, Rng};
use std::{
    process::{Child, Command},
    unimplemented,
};

mod api_account_status;
mod verifier_aggregate;

struct InMemBackend<Id: Clone> {
    es_handle: Child,
    store: EventStore<Id>,
}

impl<Id: Clone> InMemBackend<Id> {
    async fn run() -> Self {
        // Configure configure and run event store.
        let port: usize = thread_rng().gen_range(1_024, 65_535);
        let es_handle = Command::new(&format!(
            "eventstored --mem-db --disable-admin-ui --http-port {}",
            port
        ))
        .spawn()
        .unwrap();

        // Build store.
        let store = EventStoreBuilder::new(&format!("esdb://localhost:{}?tls=false", port))
            .await
            .unwrap()
            .build_store::<Id>();

        InMemBackend {
            es_handle: es_handle,
            store: store,
        }
    }
    fn store(&self) -> EventStore<Id> {
        self.store.clone()
    }
}

impl<Id: Clone> Drop for InMemBackend<Id> {
    fn drop(&mut self) {
        self.es_handle.kill().unwrap();
    }
}

/*
impl IdentityState {
    fn alice_unverified() -> Self {
        IdentityState {
            net_address:
        }
    }
}
*/
