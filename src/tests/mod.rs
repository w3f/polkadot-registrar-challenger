use crate::event::Event;
use crate::system::run_api_service;
use eventually::Subscription;
use eventually_event_store_db::{EventStore, EventStoreBuilder, EventSubscription};
use futures::{future::Join, StreamExt};
use jsonrpc_client_transports::transports::ws::connect;
use jsonrpc_client_transports::RawClient;
use jsonrpc_ws_server::Server as WsServer;
use rand::{thread_rng, Rng};
use std::convert::TryFrom;
use std::fs::canonicalize;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::time::{self, Duration};

mod api;
mod verifier_aggregate;

fn gen_port() -> usize {
    thread_rng().gen_range(1_024, 65_535)
}

struct ApiBackend {
    client: RawClient,
}

impl ApiBackend {
    async fn run() -> Self {
        //let port = gen_port();
        let port = 5_000;

        std::thread::spawn(move || {
            let mut rt = tokio_02::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let server = run_api_service(port).unwrap();
                server.wait().unwrap();
            })
        });

        // Let the server spin up.
        tokio_02::time::delay_for(Duration::from_secs(2)).await;

        // Create a client
        let client = connect::<RawClient>(&format!("ws://127.0.0.1:{}", port).parse().unwrap())
            .await
            .unwrap();

        ApiBackend { client: client }
    }
    fn client(&self) -> &RawClient {
        &self.client
    }
}

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
        let port: usize = gen_port();
        let handle = Command::new(canonicalize("/usr/bin/eventstored").unwrap())
            .arg("--mem-db")
            .arg("--disable-admin-ui")
            .arg("--insecure")
            .arg("--http-port")
            .arg(port.to_string())
            .arg("--int-tcp-port")
            .arg((port + 1).to_string())
            .arg("--ext-tcp-port")
            .arg((port + 2).to_string())
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
    async fn get_events(&self, id: Id) -> Vec<Event> {
        self.subscription(id)
            .await
            .resume()
            .await
            .unwrap()
            .then(|persisted| async { persisted.unwrap().take().as_json::<Event>().unwrap() })
            .take_until(time::sleep(Duration::from_secs(2)))
            .collect()
            .await
    }
}
