use crate::aggregate::VerifierAggregateId;
use crate::api::ConnectionPool;
use crate::event::{Event, IdentityVerification};
use crate::Result;
use eventually::store::Persisted;
use eventually::Projection;
use futures::future::BoxFuture;

pub struct SessionNotifier {
    connection_pool: ConnectionPool,
}

impl Projection for SessionNotifier {
    type SourceId = VerifierAggregateId;
    type Event = Event<IdentityVerification>;
    type Error = failure::Error;

    fn project(&mut self, event: Persisted<Self::SourceId, Self::Event>) -> BoxFuture<Result<()>> {
        unimplemented!()
    }
}
