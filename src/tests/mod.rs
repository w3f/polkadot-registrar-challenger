use crate::event::Event;
use crate::{
    aggregate::{
        verifier::{VerifierAggregateId, VerifierAggregateSnapshotsId, VerifierCommand},
        Repository,
    },
    event::{ExternalMessage, ExternalOrigin},
    manager::{ChallengeStatus, ExpectedMessage, IdentityFieldType, IdentityState, Validity},
};
use crate::{api::ConnectionPool, manager::IdentityManager};
use eventstore::Client;
use futures::{future::Join, FutureExt, Stream, StreamExt};
use hmac::digest::generic_array::typenum::Exp;
use jsonrpc_client_transports::transports::ws::connect;
use jsonrpc_client_transports::RawClient;
use jsonrpc_core::{Params, Value};
use jsonrpc_ws_server::Server as WsServer;
use lock_api::RwLock;
use rand::{thread_rng, Rng};
use std::convert::TryFrom;
use std::fs::canonicalize;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};

mod aggregate_verifier;

fn gen_port() -> usize {
    thread_rng().gen_range(1_024, 65_535)
}

struct IdentityInjector {}

impl IdentityInjector {
    fn inject_message() {}
    fn inject_judgement_request() {}
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
