use crate::adapters::email::SmtpImapClientBuilder;
use crate::adapters::matrix::MatrixClient;
use crate::adapters::twitter::TwitterBuilder;
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::aggregate::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};
use crate::api::{ConnectionPool, PublicRpc, PublicRpcApi};
use crate::event::{Event, EventType, ExternalMessage};
use crate::projection::SessionNotifier;
use crate::{EmailConfig, MatrixConfig, Result, TwitterConfig};
use async_channel::Receiver;
use futures::stream::StreamExt;
use jsonrpc_pubsub::{PubSubHandler, Session};
use jsonrpc_ws_server::{RequestContext, Server as WsServer, ServerBuilder};
use std::sync::Arc;

/*
pub async fn run_session_notifier(
    pool: ConnectionPool,
    subscription: EventSubscription<VerifierAggregateId>,
) {
    let projection = Arc::new(tokio_02::sync::RwLock::new(SessionNotifier::with_pool(
        pool,
    )));
    let mut projector = Projector::new(projection, subscription);

    // TODO: Consider exiting the entire program if this returns error.
    projector
        .run()
        .await
        .map_err(|err| {
            error!("Session notifier exited: {:?}", err);
            err
        })
        .map_err(|err| {
            println!(">>>> {:?}", err);
            err
        })
        .unwrap();
}

#[allow(dead_code)]
/// Must be started in a tokio v0.2 runtime context.
pub fn run_api_service(pool: ConnectionPool, port: usize) -> Result<WsServer> {
    let mut io = PubSubHandler::default();
    io.extend_with(PublicRpcApi::with_pool(pool).to_delegate());

    // TODO: Might consider setting `max_connections`.
    let handle = ServerBuilder::with_meta_extractor(io, |context: &RequestContext| {
        Arc::new(Session::new(context.sender().clone()))
    })
    .start(&format!("0.0.0.0:{}", port).parse()?)?;

    Ok(handle)
}

#[allow(dead_code)]
pub async fn run_verifier_subscription(
    subscription: EventSubscription<MessageWatcherId>,
    store: EventStore<VerifierAggregateId>,
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
