use crate::aggregate::Error;
use eventstore::{Client, EventData, ExpectedVersion, ReadResult, RecordedEvent, ResolvedEvent};
use futures::join;
use futures::TryStreamExt;
use std::convert::{TryFrom, TryInto};
use std::error::Error as StdError;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

mod identity_change_notifier;
pub use identity_change_notifier::SessionNotifier;

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
    pub fn new(projection: Arc<RwLock<P>>, client: Client) -> Self {
        Projector {
            projection: projection,
            client: client,
            latest_revision: Arc::new(RwLock::new(0)),
        }
    }
    pub async fn run_blocking(self) {
        let projection = self.projection;
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

        let _ = handle.await.unwrap();
        error!(
            "Projection for stream '{}' has exited unexpectedly",
            <P as Projection>::Id::default().as_ref()
        );
    }
}
