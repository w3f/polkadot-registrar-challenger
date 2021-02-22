use crate::adapters::matrix::MatrixClient;
use crate::adapters::twitter::TwitterBuilder;
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::aggregate::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};
use crate::api::{ConnectionPool, PublicRpc, PublicRpcApi};
use crate::event::{Event, EventType, ExternalMessage};
use crate::projection::{Projector, SessionNotifier};
use crate::{adapters::email::SmtpImapClientBuilder, manager::IdentityManager};
use crate::{EmailConfig, MatrixConfig, Result, TwitterConfig};
use async_channel::Receiver;
use eventstore::Client;
use futures::join;
use futures::stream::StreamExt;
use jsonrpc_pubsub::{PubSubHandler, Session};
use jsonrpc_ws_server::{RequestContext, Server as WsServer, ServerBuilder};
use std::sync::Arc;
use tokio::time::{self, Duration};

// TODO: Maybe rename `client` to `store`?
pub async fn run_rpc_api_service_blocking(
    pool: ConnectionPool,
    rpc_port: usize,
    client: Client,
    manager: Arc<parking_lot::RwLock<IdentityManager>>,
) {
    // `ConnectionPool` uses `Arc` underneath.
    let t_pool = pool.clone();

    // Start the session notifier. It listens to identity state changes from the
    // eventstore and sends all updates to the RPC API, in order to inform
    // users.
    let t_manager = Arc::clone(&manager);
    let handle1 = tokio::spawn(async move {
        let projection = Arc::new(tokio::sync::RwLock::new(SessionNotifier::new(
            pool, t_manager,
        )));
        let projector = Projector::new(projection, client.clone());
        projector.run_blocking().await;
    });

    // Start the RPC service in its own OS thread, since it requires a tokio
    // v0.2 runtime.
    let t_manager = Arc::clone(&manager);
    std::thread::spawn(move || {
        let mut rt = tokio_02::runtime::Runtime::new().unwrap();
        let listen_on = format!("127.0.0.1:{}", rpc_port);

        rt.block_on(async move {
            let mut io = PubSubHandler::default();
            io.extend_with(PublicRpcApi::new(t_pool, t_manager).to_delegate());

            // TODO: Might consider setting `max_connections`.
            let handle = ServerBuilder::with_meta_extractor(io, |context: &RequestContext| {
                Arc::new(Session::new(context.sender().clone()))
            })
            // TODO: Remove this
            .start(&listen_on.parse()?)?;

            handle.wait().unwrap();
            error!("The RPC API server exited unexpectedly");

            Result::Ok(())
        })
        .map_err(|err| {
            error!("Failed to start RPC API server: {:?}", err);
            err
        })
        .unwrap();
    });

    handle1
        .await
        .map_err(|err| {
            error!("The RPC API session notifier exited unexpectedly");
            err
        })
        .unwrap();
}

/*
#[allow(dead_code)]
pub async fn run_verifier_subscription(
    client: Client,
) -> Result<()> {
    info!("Starting message subscription");

    let repository = Repository::new(VerifierAggregate.into(), store);

    subscription
        .resume()
        .await?
        .for_each(|persisted| async {
            let event: Event = if let Ok(persisted) = persisted {
                if let Ok(event) = persisted.take().as_json() {
                    event
                } else {
                    error!("Failed to parse event from stream");
                    return ();
                }
            } else {
                error!("Failed to acquire event from stream");
                return ();
            };

            let mut root = repository.get(VerifierAggregateId).await.unwrap();

            // Verify message with the aggregate.
            match event.body {
                EventType::ExternalMessage(message) => {
                    let _ = root
                        .handle(VerifierCommand::VerifyMessage(message))
                        .await
                        .map_err(|err| error!("Failed to verify message with aggregate: {}", err));
                }
                _ => warn!("Received unexpected message"),
            }

            ()
        })
        .await;

    Ok(())
}

#[allow(dead_code)]
async fn run_matrix_listener(
    config: MatrixConfig,
    store: EventStore<MessageWatcherId>,
) -> Result<()> {
    info!("Configuring Matrix client");

    let (client, recv) = MatrixClient::new(
        &config.homeserver,
        &config.username,
        &config.password,
        &config.db_path,
    )
    .await?;

    info!("Starting Matrix client");
    client.start().await;

    messages_event_loop(store, recv, "email").await
}

#[allow(dead_code)]
async fn run_email_listener(
    config: EmailConfig,
    store: EventStore<MessageWatcherId>,
) -> Result<()> {
    info!("Configuring email client");

    let (client, recv) = SmtpImapClientBuilder::new()
        .email_server(config.smtp_server)
        .imap_server(config.imap_server)
        .email_inbox(config.inbox)
        .email_user(config.user)
        .email_password(config.password)
        .request_interval(config.request_interval)
        .build()?;

    info!("Starting email client");
    client.start().await;

    messages_event_loop(store, recv, "email").await
}

#[allow(dead_code)]
async fn run_twitter_listener(
    config: TwitterConfig,
    store: EventStore<MessageWatcherId>,
) -> Result<()> {
    info!("Configuring Twitter client");

    let (mut client, recv) = TwitterBuilder::new()
        .consumer_key(config.api_key)
        .consumer_secret(config.api_secret)
        .token(config.token)
        .token_secret(config.token_secret)
        .request_interval(config.request_interval)
        .build()?;

    info!("Starting Twitter client");
    client.start().await;

    messages_event_loop(store, recv, "Twitter").await
}

/// For each message received by an adapter, send a command to the aggregate and
/// let it handle it. This aggregate does not actually need to maintain a state.
#[allow(dead_code)]
async fn messages_event_loop<T>(
    store: EventStore<MessageWatcherId>,
    recv: Receiver<T>,
    name: &str,
) -> Result<()>
where
    T: Into<ExternalMessage>,
{
    let repository = Repository::new(MessageWatcher.into(), store);

    info!("Starting event loop for incoming {} messages", name);
    while let Ok(message) = recv.recv().await {
        let mut root = repository.get(MessageWatcherId).await.unwrap();

        let _ = root
            .handle(MessageWatcherCommand::AddMessage(message.into()))
            .await
            .map_err(|err| error!("Failed to add message to the aggregate: {}", err));
    }

    Err(anyhow!("The {} client has shut down", name))
}
*/
