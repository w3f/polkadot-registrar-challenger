use crate::account_fetch::AccountFetch;
use crate::manager::{IdentityState, NetworkAddress};
use crate::system::run_api_service;
use crate::Result;
use jsonrpc_ws_server::Server as WsServer;
use rand::{thread_rng, Rng};
use std::process::{Child, Command};
use std::sync::Weak;

mod api_account_status;

struct InMemBackend {
    es_handle: Child,
    es_connection: String,
    ws_handle: WsServer,
    ws_connection: String,
}

impl InMemBackend {
    fn run() -> Self {
        // Configure configure and run event store.
        let port: usize = thread_rng().gen_range(1_024, 65_535);
        let es_connection = format!(
            "eventstored --mem-db --disable-admin-ui --http-port {}",
            port
        );

        let es_handle = Command::new(&es_connection).spawn().unwrap();

        // Configure configure and run websocket server.
        let port: usize = thread_rng().gen_range(1_024, 65_535);
        let ws_connection = format!("0.0.0.0:{}", port);

        unimplemented!();
        //let ws_handle = run_api_service::<TestAccounts>(port).unwrap();

        /*
        InMemBackend {
            es_handle: es_handle,
            es_connection: es_connection,
            ws_handle: ws_handle,
            ws_connection: ws_connection,
        }
        */
    }
    fn event_store_connection(&self) -> &str {
        &self.es_connection
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
    fn fetch_account_state(
        net_address: &NetworkAddress,
        handle: Weak<()>,
    ) -> Result<Option<IdentityState>> {
        unimplemented!()
    }
}
