use crate::adapters::matrix::MatrixClient;
use crate::adapters::twitter::TwitterBuilder;
use crate::adapters::{email::SmtpImapClientBuilder, twitter::ReceivedMessageContext};
use crate::aggregate::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};
use crate::event::ExternalMessage;
use crate::{EmailConfig, MatrixConfig, Result, TwitterConfig};
use async_channel::Receiver;
use eventually::aggregate::AggregateRootBuilder;
use eventually::Repository;
use eventually_event_store_db::EventStore;
use lettre_email::Email;

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
