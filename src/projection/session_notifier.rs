use crate::aggregate::VerifierAggregateId;
use crate::api::ConnectionPool;
use crate::event::{Event, IdentityVerification};
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
    type Event = Event<IdentityVerification>;
    type Error = failure::Error;

    fn project(&mut self, event: Persisted<Self::SourceId, Self::Event>) -> BoxFuture<Result<()>> {
        let fut = async move {
            let event = event.take();
            let body = event.body_ref();

            self.connection_pool
                .acquire_lock()
                .sender(&body.net_address)
                .map(|sender| sender.send(Params::None.parse().unwrap()));

            Ok(())
        };

        Box::pin(fut)
    }
}
