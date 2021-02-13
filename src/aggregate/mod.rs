use eventstore::{Client, EventData, ReadResult, RecordedEvent, ResolvedEvent};
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

// Expose publicly
pub use message_watcher::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};

pub async fn with_snapshot_service<S>(client: Client) -> Result<Arc<RwLock<S>>, anyhow::Error>
where
    S: 'static + Send + Sync + Snapshot + Default,
    <S as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <S as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <S as Snapshot>::Error: 'static + Send + Sync,
{
    let stateful = Arc::new(RwLock::new(S::default()));
    let service = Snapshoter::new(Arc::clone(&stateful), client).await?;
    Ok(stateful)
}

pub async fn run_projection_with_snapshot_service<P>(client: Client) -> Result<(), anyhow::Error>
where
    P: 'static + Send + Sync + Projection + Snapshot + Default,
    <P as Projection>::Id: Send + Sync + Default + AsRef<str>,
    <P as Projection>::Event: Send + Sync + TryFrom<RecordedEvent>,
    <P as Projection>::Error: 'static + Send + Sync + StdError,
    <P as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <P as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <P as Snapshot>::Error: 'static + Send + Sync,
{
    let projection = Arc::new(RwLock::new(P::default()));
    let service = Snapshoter::new(Arc::clone(&projection), client.clone()).await?;
    let projector = Projector::new(projection, client);

    join!(service.run_blocking(), projector.run_blocking());
    Ok(())
}

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
    projection: Arc<RwLock<P>>,
    client: Client,
    latest_revision: Arc<RwLock<u64>>,
}

impl<P> Projector<P>
where
    P: 'static + Send + Sync + Projection + Default,
    <P as Projection>::Id: Send + Sync + Default + AsRef<str>,
    <P as Projection>::Event: Send + Sync + TryFrom<RecordedEvent>,
    <P as Projection>::Error: 'static + Send + Sync + StdError,
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

            let mut stream = subscribe.execute_event_appeared_only().await?;

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
                            (*projection.write().await).project(event).await?;
                        } else {
                            warn!("Did not receive a recorded event");
                        }
                    }
                    _ => {}
                }
            }

            Result::<(), anyhow::Error>::Ok(())
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
pub trait Snapshot {
    type Id;
    type State;
    type Error;

    fn qualifies(&self) -> bool;
    async fn snapshot(&self) -> Self::State;
    async fn restore(state: Self::State) -> Self;
}

pub struct Snapshoter<S> {
    // The stateful type, such as an aggregate or a projection.
    stateful: Arc<RwLock<S>>,
    client: Client,
}

impl<S> Snapshoter<S>
where
    S: 'static + Send + Sync + Snapshot,
    <S as Snapshot>::Id: Send + Sync + Default + AsRef<str>,
    <S as Snapshot>::State: Send + Sync + TryInto<EventData> + TryFrom<RecordedEvent>,
    <S as Snapshot>::Error: 'static + Send + Sync,
{
    async fn new(stateful: Arc<RwLock<S>>, client: Client) -> Result<Self, anyhow::Error> {
        info!(
            "Restoring snapshot from {}",
            <S as Snapshot>::Id::default().as_ref()
        );

        match client
            .read_stream(<S as Snapshot>::Id::default())
            .backward()
            .execute(1)
            .await?
        {
            ReadResult::Ok(mut stream) => {
                while let Some(resolved) = stream.try_next().await? {
                    info!("Snapshot found, restoring");

                    if let Some(recorded) = resolved.event {
                        let mut s = stateful.write().await;
                        *s =
                            S::restore(<S as Snapshot>::State::try_from(recorded).map_err(
                                |_| anyhow!("Failed to convert snapshot into native type"),
                            )?)
                            .await;
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
        })
    }
    async fn run_blocking(self) {
        let stateful = self.stateful;
        let client = self.client;

        let handle = tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(1));
            loop {
                interval.tick().await;

                {
                    if !stateful.read().await.qualifies() {
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
                    .await?;
            }

            #[allow(unreachable_code)]
            Result::<(), anyhow::Error>::Ok(())
        });

        let msg = format!(
            "Snapshoter for stream '{}' has exited unexpectedly",
            <S as Snapshot>::Id::default().as_ref()
        );
        error!("{}", &msg);
        let _ = handle.await.expect(&msg);
    }
}
