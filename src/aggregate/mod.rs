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
mod message_watcher;
pub mod verifier;

// TODO: Add this to crate root.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct Error(#[from] anyhow::Error);

// Expose publicly
pub use message_watcher::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};

pub async fn with_snapshot_service<S>(
    client: Client,
    snapshot_every: usize,
) -> Result<Arc<RwLock<S>>, Error>
where
    S: 'static + Send + Sync + Snapshot + Default,
    <S as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <S as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <S as Snapshot>::Error: 'static + Send + Sync + Debug,
{
    let stateful = Arc::new(RwLock::new(S::default()));
    let service = Snapshoter::new(Arc::clone(&stateful), client, snapshot_every).await?;
    Ok(stateful)
}

pub async fn run_projection_with_snapshot_service<P>(
    client: Client,
    snapshot_every: usize,
) -> Result<(), Error>
where
    P: 'static + Send + Sync + Projection + Snapshot + Default,
    <P as Projection>::Id: Send + Sync + Default + AsRef<str>,
    <P as Projection>::Event: Send + Sync + TryFrom<RecordedEvent>,
    <P as Projection>::Error: 'static + Send + Sync + StdError,
    <P as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <P as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <P as Snapshot>::Error: 'static + Send + Sync + Debug,
{
    let projection = Arc::new(RwLock::new(P::default()));
    let service = Snapshoter::new(Arc::clone(&projection), client.clone(), snapshot_every).await?;
    let projector = Projector::new(projection, client);

    join!(service.run_blocking(), projector.run_blocking());
    Ok(())
}

#[async_trait]
pub trait Aggregate {
    type Id;
    type Event;
    type State;
    type Command;
    type Error;

    fn state(&self) -> &Self::State;
    async fn apply(&mut self, event: Self::Event) -> Result<(), Self::Error>;
    async fn handle(&self, command: Self::Command)
        -> Result<Option<Vec<Self::Event>>, Self::Error>;
}

pub struct Repository<A> {
    aggregate: A,
    client: Client,
    snapshot_every: usize,
}

impl<A> Repository<A>
where
    A: 'static + Send + Sync + Aggregate + Snapshot + Default,
    <A as Aggregate>::Id: Send + Sync + AsRef<str> + Default,
    <A as Aggregate>::Event: Send + Sync + TryInto<EventData> + Clone,
    <A as Aggregate>::Error: 'static + Send + Sync + Debug,
    <A as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <A as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <A as Snapshot>::Error: 'static + Send + Sync + Debug,
{
    pub async fn new_with_snapshot_service(
        client: Client,
        snapshot_every: usize,
    ) -> Result<Self, anyhow::Error> {
        let mut aggregate = A::default();
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
            snapshot_every: snapshot_every,
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
        if self.aggregate.qualifies(self.snapshot_every) {
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
pub trait Projection {
    type Id;
    type Event;
    type Error;

    async fn project(&mut self, event: Self::Event) -> Result<(), Self::Error>;
}

pub struct Projector<P> {
    projection: Arc<RwLock<P>>,
    client: Client,
    latest_revision: Arc<RwLock<u64>>,
}

impl<P> Projector<P>
where
    P: 'static + Send + Sync + Projection + Default,
    <P as Projection>::Id: Send + Sync + Default + AsRef<str>,
    <P as Projection>::Event: Send + Sync + TryFrom<RecordedEvent>,
    <P as Projection>::Error: 'static + Send + Sync + Debug,
{
    fn new(projection: Arc<RwLock<P>>, client: Client) -> Self {
        Projector {
            projection: projection,
            client: client,
            latest_revision: Arc::new(RwLock::new(0)),
        }
    }
    async fn run_blocking(self) {
        let mut projection = self.projection;
        let client = self.client;
        let latest_revision = Arc::clone(&self.latest_revision);

        let handle = tokio::spawn(async move {
            let subscribe = client
                .subscribe_to_stream_from(<P as Projection>::Id::default())
                .start_position(*latest_revision.read().await);

            let mut stream = subscribe
                .execute_event_appeared_only()
                .await
                .map_err(|err| anyhow!("failed to open stream to projection: {:?}", err))?;

            while let Ok(event) = stream.try_next().await {
                match event {
                    Some(resolved) => {
                        if let Some(recorded) = resolved.event {
                            *latest_revision.write().await = recorded.revision;

                            // Parse event.
                            let event =
                                <P as Projection>::Event::try_from(recorded).map_err(|_| {
                                    anyhow!("failed to convert eventstore event into native type")
                                })?;

                            // Project event.
                            (*projection.write().await)
                                .project(event)
                                .await
                                .map_err(|err| anyhow!("failed to run projection: {:?}", err))?;
                        } else {
                            warn!("Did not receive a recorded event");
                        }
                    }
                    _ => {}
                }
            }

            Result::<(), Error>::Ok(())
        });

        let msg = format!(
            "Projection for stream '{}' has exited unexpectedly",
            <P as Projection>::Id::default().as_ref()
        );
        error!("{}", &msg);
        let _ = handle.await.expect(&msg);
    }
}

#[async_trait]
pub trait Snapshot: Sized {
    type Id;
    type State;
    type Error;

    fn qualifies(&self, every: usize) -> bool;
    async fn snapshot(&self) -> Self::State;
    async fn restore(state: Self::State) -> Result<Self, Self::Error>;
}

// TODO: Deprecate this.
pub struct Snapshoter<S> {
    // The stateful type, such as an aggregate or a projection.
    stateful: Arc<RwLock<S>>,
    client: Client,
    snapshot_every: usize,
}

impl<S> Snapshoter<S>
where
    S: 'static + Send + Sync + Snapshot,
    <S as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <S as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <S as Snapshot>::Error: 'static + Send + Sync + Debug,
{
    async fn new(
        stateful: Arc<RwLock<S>>,
        client: Client,
        snapshot_every: usize,
    ) -> Result<Self, Error> {
        info!(
            "Restoring snapshot from {}",
            <S as Snapshot>::Id::default().as_ref()
        );

        match client
            .read_stream(<S as Snapshot>::Id::default())
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
                    info!("Snapshot found, restoring");

                    if let Some(recorded) = resolved.event {
                        let mut s = stateful.write().await;
                        *s =
                            S::restore(<S as Snapshot>::State::try_from(recorded).map_err(
                                |_| anyhow!("failed to convert snapshot into native type"),
                            )?)
                            .await
                            .map_err(|err| anyhow!("failed to restore from snapshot: {:?}", err))?;
                    }
                }
            }
            ReadResult::StreamNotFound(name) => {
                warn!(
                    "No snapshots found on stream '{}', starting from scratch",
                    name
                );
            }
        }

        Ok(Snapshoter {
            stateful: stateful,
            client: client,
            snapshot_every: snapshot_every,
        })
    }
    async fn run_blocking(self) {
        let stateful = self.stateful;
        let client = self.client;
        let snapshot_every = self.snapshot_every;

        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;

                {
                    if !stateful.read().await.qualifies(snapshot_every) {
                        continue;
                    }
                }

                let state = stateful.read().await.snapshot().await;
                let event = state.try_into().map_err(|_| {
                    anyhow!("Failed to convert native snapshot into evenstore event")
                })?;

                let _ = client
                    .write_events(<S as Snapshot>::Id::default())
                    .send_event(event)
                    .await
                    .map_err(|err| anyhow!("failed to send snapshot to the eventstore: {:?}"))?;
            }

            #[allow(unreachable_code)]
            Result::<(), Error>::Ok(())
        });

        let msg = format!(
            "Snapshoter for stream '{}' has exited unexpectedly",
            <S as Snapshot>::Id::default().as_ref()
        );
        error!("{}", &msg);
        let _ = handle.await.expect(&msg);
    }
}
