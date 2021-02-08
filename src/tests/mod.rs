use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId};
use crate::event::Event;
use crate::manager::{
    FieldAddress, FieldStatus, IdentityAddress, IdentityState, NetworkAddress,
    RegistrarIdentityField,
};
use crate::system::run_api_service;
use eventually::Subscription;
use eventually_event_store_db::{EventStore, EventStoreBuilder, EventSubscription};
use futures::StreamExt;
use jsonrpc_ws_server::Server as WsServer;
use rand::{thread_rng, Rng};
use std::convert::TryFrom;
use std::fs::canonicalize;
use std::process::Stdio;
use tokio::process::{Child, Command};

mod api_account_status;
mod verifier_aggregate;

struct InMemBackend<Id: Clone> {
    store: EventStore<Id>,
    port: usize,
    _handle: Child,
}

impl<Id> InMemBackend<Id>
where
    Id: 'static + Send + Sync + Eq + TryFrom<String> + Clone + AsRef<str>,
{
    async fn run() -> Self {
        // Configure and spawn the event store in a background process.
        //let port: usize = thread_rng().gen_range(1_024, 65_535);
        let port = 4000;
        let handle = Command::new(canonicalize("/usr/bin/eventstored").unwrap())
            .arg("--mem-db")
            //.arg("--disable-admin-ui")
            .arg("--insecure")
            .arg("--http-port")
            .arg(port.to_string())
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
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
            port: port,
            _handle: handle,
        }
    }
    fn store(&self) -> EventStore<Id> {
        self.store.clone()
    }
    async fn subscription(&self, id: Id) -> EventSubscription<Id> {
        EventStoreBuilder::new(&format!("esdb://localhost:{}?tls=false", self.port))
            .await
            .unwrap()
            .verify_connection(5)
            .await
            .unwrap()
            .build_persistant_subscription::<Id>(id, "test_subscription11")
    }
    async fn take_events(&self, id: Id, take: usize) -> Vec<Event> {
        self.subscription(id)
            .await
            .resume()
            .await
            .unwrap()
            .then(|persisted| async { persisted.unwrap().take().as_json::<Event>().unwrap() })
            .take(take)
            .collect()
            .await
    }
}
