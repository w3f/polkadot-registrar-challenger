use crate::aggregate::VerifierAggregateId;
use crate::api::ConnectionPool;
use crate::event::{Event, StateWrapper};
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
    type Event = Event<StateWrapper>;
    type Error = failure::Error;

    fn project(&mut self, event: Persisted<Self::SourceId, Self::Event>) -> BoxFuture<Result<()>> {
        let fut = async move {
            let event = event.take();
            let body = event.body_ref();

            if let Some(sender) = self.connection_pool.sender(&body.state.net_address) {
                match sender.send(event).await {
                    Ok(_) => {}
                    Err(_) => {}
                }
            }

            Ok(())
        };

        Box::pin(fut)
    }
}
