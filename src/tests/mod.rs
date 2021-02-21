use crate::event::Event;
use crate::system::run_rpc_api_service_blocking;
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
mod rpc_api_service;

/// Generates (kind of) random events. Primarily used for manual testing in
/// order to see whether the front end can process new messages and display
/// notifications.
#[tokio::test]
#[ignore]
async fn generate_random_data() {
    use crate::aggregate::verifier::{VerifierAggregate, VerifierCommand};
    use crate::aggregate::Repository;
    use crate::event::{ExternalMessage, ExternalOrigin};
    use crate::manager::{
        ChallengeStatus, ExpectedMessage, FieldAddress, FieldStatus, IdentityField,
        IdentityFieldType, IdentityState, RegistrarIdentityField,
    };

    fn random(between: usize) -> usize {
        thread_rng().gen_range(0, between)
    }

    env_logger::init();

    // Setup backend. Keep a handle to the state for being able to wipe it
    // later on (see infinite loop below).
    let be = InMemBackend::run().await;
    let store = be.store();
    let manager = Arc::new(parking_lot::RwLock::new(IdentityManager::default()));
    ApiBackend::run_fixed_port(8080, store.clone(), Arc::clone(&manager)).await;

    // Setup aggregate and repository.
    // TODO: Set shared state in `VerifierAggregate` directly.
    let mut repo =
        Repository::new_with_snapshot_service(VerifierAggregate::default(), store.clone())
            .await
            .unwrap();

    // Add identities.
    let mut alice = IdentityState::alice();
    let bob = IdentityState::bob();

    // Set custom field.
    alice.fields.insert(
        IdentityFieldType::LegalName,
        FieldStatus::from({
            (
                IdentityField::LegalName(FieldAddress::from("Alice Doe".to_string())),
                // Throwaway value
                RegistrarIdentityField::display_name(),
            )
        }),
    );

    // Insert identities.
    repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
        .await
        .unwrap();

    repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
        .await
        .unwrap();

    // Run forever
    loop {
        // 10% chance to reset state.
        if random(10) <= 1 {
            repo.wipe();
            *manager.write() = IdentityManager::default();

            // Insert identities.
            repo.apply(VerifierCommand::InsertIdentity(alice.clone()))
                .await
                .unwrap();

            repo.apply(VerifierCommand::InsertIdentity(bob.clone()))
                .await
                .unwrap();

            time::sleep(Duration::from_secs(random(10) as u64)).await;
            continue;
        }

        // Chose a random field to modify.
        let field_ty = match random(3) {
            0 => IdentityFieldType::Email,
            1 => IdentityFieldType::Twitter,
            2 => IdentityFieldType::Matrix,
            _ => panic!(),
        };

        // Set challenge status.
        let expected = alice
            .fields
            .get(&field_ty)
            .map(|field| match field.challenge() {
                ChallengeStatus::ExpectMessage(challenge) => {
                    let msg = match random(2) {
                        0 => challenge.expected_message.clone(),
                        1 => ExpectedMessage::gen(),
                        _ => panic!(),
                    };

                    Some((challenge.from.inner(), msg))
                }
                ChallengeStatus::BackAndForth(challenge) => {
                    let msg = match random(101) {
                        0..=50 => challenge.expected_message.clone(),
                        51..=100 => ExpectedMessage::gen(),
                        _ => panic!(),
                    };

                    Some((challenge.from.inner(), msg))
                }
                // Ignore those.
                _ => None,
            });

        // Apply changes.
        if let Some(expected) = expected {
            if let Some((from, msg)) = expected {
                repo.apply(VerifierCommand::VerifyMessage(ExternalMessage {
                    origin: {
                        match field_ty {
                            IdentityFieldType::Email => ExternalOrigin::Email,
                            IdentityFieldType::Matrix => ExternalOrigin::Matrix,
                            IdentityFieldType::Twitter => ExternalOrigin::Twitter,
                            _ => panic!(),
                        }
                    },
                    field_address: from,
                    message: msg.into(),
                }))
                .await
                .unwrap();
            }
        }

        time::sleep(Duration::from_secs(random(10) as u64)).await;
    }
}

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
        let manager = Arc::new(parking_lot::RwLock::new(IdentityManager::default()));
        tokio::spawn(run_rpc_api_service_blocking(
            ConnectionPool::default(),
            rpc_port,
            store,
            manager,
        ));

        rpc_port
    }
    async fn run_fixed_port(
        rpc_port: usize,
        store: Client,
        manager: Arc<parking_lot::RwLock<IdentityManager>>,
    ) {
        tokio::spawn(run_rpc_api_service_blocking(
            ConnectionPool::default(),
            rpc_port,
            store,
            manager,
        ));
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
