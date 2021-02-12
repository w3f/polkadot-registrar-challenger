use eventstore::{Client, EventData, ResolvedEvent};
use futures::TryStreamExt;
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use std::fmt::Debug;

pub mod display_name;
mod message_watcher;
pub mod verifier;

// Expose publicly
pub use message_watcher::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};

use crate::event::ErrorMessage;

/// This wrapper type is required for the use of `eventually`, since
/// `anyhow::Error` does not implement `std::error::Error`.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct Error(#[from] anyhow::Error);

#[async_trait]
pub trait Aggregate {
    type Id;
    type Event;
    type Command;
    type Error;

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
    A: Aggregate + Default,
    <A as Aggregate>::Id: Send + Sync + AsRef<str> + Default,
    <A as Aggregate>::Event: Send + Sync + TryInto<EventData> + Clone,
    <A as Aggregate>::Error: 'static + Send + Sync + Debug + StdError,
{
    fn new(client: Client) -> Self {
        Repository {
            aggregate: Default::default(),
            client: client,
        }
    }
    async fn apply(&mut self, command: <A as Aggregate>::Command) -> Result<(), anyhow::Error> {
        if let Some(events) = self.aggregate.handle(command).await? {
            let to_store = events.clone();

            // Send events to the store.
            self.client
                .write_events(<A as Aggregate>::Id::default())
                .send_iter(
                    to_store
                        .into_iter()
                        .map(|event| {
                            event.try_into().map_err(|_| {
                                anyhow!("Failed to convert event into eventstore native format",)
                            })
                        })
                        .collect::<Result<Vec<EventData>, anyhow::Error>>()?,
                )
                .await??;

            // Apply events locally.
            for event in events {
                self.aggregate.apply(event).await?;
            }
        }

        Ok(())
    }
}

#[async_trait]
pub trait Projection {
    type Id;
    type Event;
    type Error;

    async fn project(&mut self, event: Self::Event) -> Result<(), Self::Error>;
}

pub struct Projector<P> {
    projection: P,
    client: Client,
}

impl<P> Projector<P>
where
    P: 'static + Send + Projection + Default,
    <P as Projection>::Id: Send + Sync + Default + AsRef<str>,
    <P as Projection>::Event: Send + Sync + TryFrom<ResolvedEvent>,
    <P as Projection>::Error: 'static + Send + Sync + StdError,
{
    fn new(client: Client) -> Self {
        Projector {
            projection: Default::default(),
            client: client,
        }
    }
    async fn run(self) -> Result<(), anyhow::Error> {
        let mut projection = self.projection;
        let client = self.client;

        tokio::spawn(async move {
            let subscribe = client.subscribe_to_stream_from(<P as Projection>::Id::default());
            let mut stream = subscribe.execute_event_appeared_only().await?;

            while let Ok(event) = stream.try_next().await {
                match event {
                    Some(resolved) => {
                        // Parse event.
                        let event = <P as Projection>::Event::try_from(resolved).map_err(|_| {
                            anyhow!("Failed to convert eventstore native type to local time")
                        })?;

                        // Project event.
                        projection.project(event).await?;
                    }
                    _ => {}
                }
            }

            Result::<(), anyhow::Error>::Ok(())
        });

        Ok(())
    }
}
