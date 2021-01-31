use crate::adapters::matrix::MatrixClient;
use crate::aggregate::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};
use crate::{MatrixConfig, Result};
use eventually::aggregate::AggregateRootBuilder;
use eventually::Repository;
use eventually_event_store_db::EventStore;

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
