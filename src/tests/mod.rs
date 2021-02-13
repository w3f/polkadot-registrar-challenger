use crate::aggregate::verifier::VerifierAggregateId;
use crate::api::ConnectionPool;
use crate::event::Event;
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
    async fn run(pool: ConnectionPool) -> Self {
        let port = gen_port();

        std::thread::spawn(move || {
            let mut rt = tokio_02::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                //let server = run_api_service(pool, port).unwrap();
                //server.wait().unwrap();
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
