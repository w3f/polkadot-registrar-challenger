use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId};
use crate::manager::{
    FieldAddress, FieldStatus, IdentityAddress, IdentityState, NetworkAddress,
    RegistrarIdentityField,
};
use crate::system::run_api_service;
use eventually_event_store_db::{EventStore, EventStoreBuilder};
use jsonrpc_ws_server::Server as WsServer;
use rand::{thread_rng, Rng};
use std::fs::canonicalize;
use tokio::process::{Command, Child};
use std::process::Stdio;

mod api_account_status;
mod verifier_aggregate;

struct InMemBackend<Id: Clone> {
    store: EventStore<Id>,
    _handle: Child,
}

impl<Id: Clone> InMemBackend<Id> {
    async fn run() -> Self {
        println!("FRIST");

        // Configure and spawn the event store in a background process.
        let port: usize = thread_rng().gen_range(1_024, 65_535);
        let handle = Command::new(canonicalize("/usr/bin/eventstored").unwrap())
            .arg("--mem-db")
            .arg("--disable-admin-ui")
            .arg("--insecure")
            .arg("--http-port")
            .arg(port.to_string())
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();

        // Build store.
        let store = EventStoreBuilder::new(&format!("esdb://localhost:{}?tls=false", port))
            .await
            .unwrap()
            .verify_connection(5)
            .await
            .unwrap()
            .build_store::<Id>();

        InMemBackend {
            store: store,
            _handle: handle,
        }
    }
    fn store(&self) -> EventStore<Id> {
        self.store.clone()
    }
}
