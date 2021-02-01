use crate::adapters::email::SmtpImapClientBuilder;
use crate::adapters::matrix::MatrixClient;
use crate::aggregate::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};
use crate::{EmailConfig, MatrixConfig, Result};
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

    let repository = Repository::new(MessageWatcher.into(), store);

    // For each message received, send a command to the aggregate and let it
    // handle it. This aggregate does not actually need to maintain a state.
    info!("Starting event loop for incoming Matrix messages");
    while let Ok(message) = recv.recv().await {
        // TODO: Why does Repository::get() require a parameter?
        let mut root = repository.get(MessageWatcherId).await.unwrap();

        let _ = root
            .handle(MessageWatcherCommand::AddMessage(message.into()))
            .await
            .map_err(|err| error!("Failed to add message to the aggregate: {}", err));
    }

    Err(anyhow!("The Matrix client has shut down"))
}

async fn run_email_listener(
    config: EmailConfig,
    store: EventStore<MessageWatcherId>,
) -> Result<()> {
    let (client, recv) = SmtpImapClientBuilder::new()
        .email_server(config.smtp_server)
        .imap_server(config.imap_server)
        .email_inbox(config.inbox)
        .email_user(config.user)
        .email_password(config.password)
        .request_interval(config.request_interval)
        .build()?;

    info!("Starting Email client");
    client.start().await;

    let repository = Repository::new(MessageWatcher.into(), store);

    info!("Starting event loop for incoming Email messages");
    while let Ok(message) = recv.recv().await {
        let mut root = repository.get(MessageWatcherId).await.unwrap();

        let _ = root
            .handle(MessageWatcherCommand::AddMessage(message.into()))
            .await
            .map_err(|err| error!("Failed to add message to the aggregate: {}", err));
    }

    Err(anyhow!("The Email client has shut down"))
}
