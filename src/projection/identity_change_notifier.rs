use crate::aggregate::verifier::VerifierAggregateId;
use crate::aggregate::Error;
use crate::api::ConnectionPool;
use crate::event::{Event, EventType};
use eventually::store::Persisted;
use eventually::Projection;
use eventually_event_store_db::GenericEvent;
use futures::future::BoxFuture;

#[derive(Default)]
pub struct SessionNotifier {
    connection_pool: ConnectionPool,
}

impl SessionNotifier {
    pub fn with_pool(pool: ConnectionPool) -> Self {
        SessionNotifier {
            connection_pool: pool,
        }
    }
}

impl Projection for SessionNotifier {
    type SourceId = VerifierAggregateId;
    type Event = GenericEvent;
    type Error = Error;

    fn project(
        &mut self,
        event: Persisted<Self::SourceId, Self::Event>,
    ) -> BoxFuture<Result<(), Error>> {
        let fut = async move {
            let event = event
                .take()
                .as_json::<Event>()
                .map_err(|err| anyhow!("Failed to deserialize event: {:?}", err))?;

            let net_address = match event.body {
                EventType::FieldStatusVerified(ref field_status) => &field_status.net_address,
                EventType::IdentityFullyVerified(ref verified) => &verified.net_address,
                _ => return Ok(()),
            };

            if let Some(notifier) = self.connection_pool.notify_net_address(net_address) {
                if let Err(_) = notifier.send(event).await {
                    info!("All connections to RPC sessions are closed");
                }
            }

            Ok(())
        };

        Box::pin(fut)
    }
}
