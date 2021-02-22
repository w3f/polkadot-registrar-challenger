use eventstore::{Client, EventData, ExpectedVersion, ReadResult, RecordedEvent, ResolvedEvent};
use futures::join;
use futures::TryStreamExt;
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

pub mod display_name;
pub mod message_watcher;
pub mod verifier;

// TODO: Add this to crate root.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct Error(#[from] anyhow::Error);

// Expose publicly
pub use message_watcher::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};

#[async_trait]
pub trait Aggregate {
    type Id;
    type Event;
    type State;
    type Command;
    type Error;

    // TODO: Remove
    #[cfg(test)]
    fn wipe(&mut self);
    fn state(&self) -> &Self::State;
    async fn apply(&mut self, event: Self::Event) -> Result<(), Self::Error>;
    async fn handle(&self, command: Self::Command)
        -> Result<Option<Vec<Self::Event>>, Self::Error>;
}

pub struct Repository<A> {
    aggregate: A,
    client: Client,
}

impl<A> Repository<A>
where
    A: 'static + Send + Sync + Aggregate + Snapshot,
    <A as Aggregate>::Id: Send + Sync + AsRef<str> + Default,
    <A as Aggregate>::Event: Send + Sync + TryInto<EventData> + Clone,
    <A as Aggregate>::Error: 'static + Send + Sync + Debug,
    <A as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <A as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <A as Snapshot>::Error: 'static + Send + Sync + Debug,
{
    // TODO: Remove
    #[cfg(test)]
    pub fn wipe(&mut self) {
        self.aggregate.wipe()
    }
    // TODO: Rename "client" to "store".
    pub async fn new_with_snapshot_service(
        mut aggregate: A,
        client: Client,
    ) -> Result<Self, anyhow::Error> {
        let mut snapshot_found = false;

        // Check if there is a snapshot available in the eventstore.
        match client
            .read_stream(<A as Snapshot>::Id::default())
            .start_from_end_of_stream()
            .execute(1)
            .await
            .map_err(|err| {
                anyhow!(
                    "failed to open stream to retrieve latest snapshot: {:?}",
                    err
                )
            })? {
            ReadResult::Ok(mut stream) => {
                while let Some(resolved) = stream.try_next().await.map_err(|err| {
                    anyhow!("failed to retrieve snapshot from the eventstore: {:?}", err)
                })? {
                    if let Some(recorded) = resolved.event {
                        info!("Snapshot found, restoring");

                        aggregate =
                            A::restore(<A as Snapshot>::State::try_from(recorded).map_err(
                                |_| anyhow!("failed to convert snapshot into native type"),
                            )?)
                            .await
                            .map_err(|err| anyhow!("failed to restore from snapshot: {:?}", err))?;

                        info!("Snapshot restored");
                        snapshot_found = true;
                        break;
                    }
                }
            }
            ReadResult::StreamNotFound(_) => {}
        }

        if !snapshot_found {
            warn!(
                "No snapshots found on stream '{}', starting from scratch",
                <A as Snapshot>::Id::default().as_ref()
            );
        }

        Ok(Repository {
            aggregate: aggregate,
            client: client,
        })
    }
    pub fn state(&self) -> &<A as Aggregate>::State {
        <A as Aggregate>::state(&self.aggregate)
    }
    pub async fn apply(&mut self, command: <A as Aggregate>::Command) -> Result<(), Error> {
        let events = {
            if let Some(events) = self
                .aggregate
                .handle(command)
                .await
                .map_err(|err| anyhow!("failed to handle aggregate command: {:?}", err))?
            {
                events
            } else {
                return Ok(());
            }
        };

        let to_store = events.clone();

        // Send events to the store.
        self.client
            .write_events(<A as Aggregate>::Id::default())
            .send_iter(
                to_store
                    .into_iter()
                    .map(|event| {
                        event.try_into().map_err(|_| {
                            anyhow!("Failed to convert event into eventstore native format").into()
                        })
                    })
                    .collect::<Result<Vec<EventData>, Error>>()?,
            )
            .await
            .map_err(|err| {
                anyhow!(
                    "failed to send aggregate events to the eventstore: {:?}",
                    err
                )
            })?
            .map_err(|err| {
                anyhow!(
                    "failed to send aggregate events to the eventstore: {:?}",
                    err
                )
            })?;

        // Apply events locally.
        for event in events {
            self.aggregate.apply(event).await.map_err(|err| {
                anyhow!(
                    "Failed to apply aggregate events to the local state: {:?}",
                    err
                )
            })?;
        }

        // Create a snapshot, if dictated.
        if self.aggregate.qualifies() {
            let state = self.aggregate.snapshot().await;
            let event = state
                .try_into()
                .map_err(|_| anyhow!("Failed to convert native snapshot into evenstore event"))?;

            let _ = self
                .client
                .write_events(<A as Snapshot>::Id::default())
                .send_event(event)
                .await
                .map_err(|err| anyhow!("failed to send snapshot to the eventstore: {:?}", err))?;

            info!(
                "Created snapshot on stream '{}'",
                <A as Snapshot>::Id::default().as_ref()
            );
        }

        Ok(())
    }
}

#[async_trait]
pub trait Snapshot: Sized {
    type Id;
    type State;
    type Error;

    fn qualifies(&self) -> bool;
    async fn snapshot(&self) -> Self::State;
    async fn restore(state: Self::State) -> Result<Self, Self::Error>;
}
