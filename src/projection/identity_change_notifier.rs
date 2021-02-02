use crate::aggregate::verifier::{self, VerifierAggregateId};
use crate::api::ConnectionPool;
use crate::event::{Event, EventType, StateWrapper};
use crate::Result;
use eventually::store::Persisted;
use eventually::Projection;
use futures::future::BoxFuture;
use jsonrpc_core::types::params::Params;

pub struct SessionNotifier {
    connection_pool: ConnectionPool,
}

impl Projection for SessionNotifier {
    type SourceId = VerifierAggregateId;
    type Event = Event;
    type Error = anyhow::Error;

    fn project(&mut self, event: Persisted<Self::SourceId, Self::Event>) -> BoxFuture<Result<()>> {
        let fut = async move {
            let event = event.take();
            let net_address = match event.body {
                EventType::FieldStatusVerified(ref field_status) => &field_status.net_address,
                EventType::IdentityFullyVerified(ref verified) => &verified.net_address,
                _ => {
                    warn!("Received unexpected event when notifying sessions");
                    return Ok(());
                }
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
