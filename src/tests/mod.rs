use crate::account_fetch::AccountFetch;
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId};
use crate::manager::{IdentityState, NetworkAddress};
use crate::system::run_api_service;
use crate::Result;
use eventually_event_store_db::EventStoreBuilder;
use jsonrpc_ws_server::Server as WsServer;
use rand::{thread_rng, Rng};
use std::process::{Child, Command};

mod api_account_status;

struct InMemBackend {
    es_handle: Child,
    ws_handle: WsServer,
    ws_connection: String,
}

impl InMemBackend {
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
            .build_store::<VerifierAggregateId>();

        // Configure configure and run websocket server.
        let port: usize = thread_rng().gen_range(1_024, 65_535);
        let ws_connection = format!("0.0.0.0:{}", port);
        let ws_handle = run_api_service::<TestAccounts>(store, VerifierAggregate, port).unwrap();

        InMemBackend {
            es_handle: es_handle,
            ws_handle: ws_handle,
            ws_connection: ws_connection,
        }
    }
    fn api_connection(&self) -> &str {
        &self.ws_connection
    }
}

impl Drop for InMemBackend {
    fn drop(&mut self) {
        self.es_handle.kill().unwrap();
        // ws_handle closes itself when dropped.
    }
}

#[derive(Default)]
struct TestAccounts {}

impl AccountFetch for TestAccounts {
    fn fetch_account_state(_net_address: &NetworkAddress) -> Result<Option<IdentityState>> {
        unimplemented!()
    }
}
