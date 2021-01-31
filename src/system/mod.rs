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
    let (client, recv) = MatrixClient::new(
        &config.homeserver,
        &config.username,
        &config.password,
        &config.db_path,
    )
    .await?;

    let repository = Repository::new(MessageWatcher.into(), store);

    while let Ok(message) = recv.recv().await {
        // TODO: Why does Repository::get() require a parameter?
        let mut root = repository.get(MessageWatcherId).await.unwrap();
        root.handle(MessageWatcherCommand::AddMessage(message.into()))
            .await
            .map_err(|err| error!("Failed to handle command to add message: {}", err));
    }

    Err(anyhow!("The Matrix client has shut down"))
}
