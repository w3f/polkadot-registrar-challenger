use crate::aggregate::verifier::VerifierAggregateId;
use crate::api::ConnectionPool;
use crate::event::Event;
use crate::system::run_rpc_api_service_blocking;
use eventstore::Client;
use futures::{future::Join, FutureExt, Stream, StreamExt};
use jsonrpc_client_transports::transports::ws::connect;
use jsonrpc_client_transports::RawClient;
use jsonrpc_core::{Params, Value};
use jsonrpc_ws_server::Server as WsServer;
use rand::{thread_rng, Rng};
use std::convert::TryFrom;
use std::fs::canonicalize;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};

mod aggregate_verifier;
mod rpc_api_service;

fn gen_port() -> usize {
    thread_rng().gen_range(1_024, 65_535)
}

// Must be called in a tokio v0.2 context.
struct ApiClient {
    client: RawClient,
}

impl ApiClient {
    async fn new(rpc_port: usize) -> ApiClient {
        ApiClient {
            client: connect::<RawClient>(&format!("ws://127.0.0.1:{}", rpc_port).parse().unwrap())
                .await
                .unwrap(),
        }
    }
    fn raw(&self) -> &RawClient {
        &self.client
    }
    async fn get_messages(
        &self,
        subscribe: &str,
        params: Params,
        notification: &str,
        unsubscribe: &str,
    ) -> Vec<Value> {
        self.client
            .subscribe(subscribe, params, notification, unsubscribe)
            .unwrap()
            .then(|v| async { v.unwrap() })
            .take_until(tokio_02::time::delay_for(Duration::from_secs(2)))
            .collect()
            .await
    }
}

struct ApiBackend;

impl ApiBackend {
    async fn run(store: Client) -> usize {
        let rpc_port = gen_port();
        tokio::spawn(run_rpc_api_service_blocking(
            ConnectionPool::default(),
            rpc_port,
            store,
        ));

        rpc_port
    }
}

struct InMemBackend {
    store: Client,
    port: usize,
    _handle: Child,
}

impl InMemBackend {
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

        tokio::time::sleep(Duration::from_secs(2)).await;

        // Build store.
        let store = Client::create(
            format!("esdb://localhost:{}?tls=false", port)
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();

        InMemBackend {
            store: store,
            port: port,
            _handle: handle,
        }
    }
    fn store(&self) -> Client {
        self.store.clone()
    }
    /*
    async fn run_session_notifier(&self, pool: ConnectionPool) {
        let subscription = self.subscription(VerifierAggregateId).await;
        tokio::spawn(async move {
            run_session_notifier(pool, subscription).await;
        });
    }
    */
    async fn subscription<Id>(&self, id: Id) -> impl Stream<Item = Event>
    where
        Id: Send + Sync + Eq + AsRef<str> + Default,
    {
        self.store
            .subscribe_to_stream_from(Id::default())
            .execute_event_appeared_only()
            .await
            .unwrap()
            .then(|resolved| async {
                match resolved.unwrap().event {
                    Some(recorded) => Event::try_from(recorded).unwrap(),
                    _ => panic!(),
                }
            })
    }
    async fn get_events<Id>(&self, id: Id) -> Vec<Event>
    where
        Id: Send + Sync + Eq + AsRef<str> + Default,
    {
        self.subscription(id)
            .await
            .take_until(time::sleep(Duration::from_secs(2)))
            .collect()
            .await
    }
}
