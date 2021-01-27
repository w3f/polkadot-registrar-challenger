use eventually::store::{AppendError, EventStream, Expected, Persisted, Select};
// TODO: Alias `EventStream` as `StoreEventStream`
use futures::future::BoxFuture;
use std::convert::{AsRef, TryFrom};
use std::marker::PhantomData;

mod message_watcher;
pub mod request_handler;
mod response_handler;
mod verifier;

pub use verifier::VerifierAggregateId;

type Result<T> = std::result::Result<T, EmptyStoreError>;

#[derive(Debug, Error)]
pub enum EmptyStoreError {
    #[error("EmptyStore does not support any functionality")]
    NotPermitted,
}

impl AppendError for EmptyStoreError {
    fn is_conflict_error(&self) -> bool {
        false
    }
}

pub struct EmptyStore<Id, Event> {
    _p1: PhantomData<Id>,
    _p2: PhantomData<Event>,
}

impl<Id, Event> eventually::EventStore for EmptyStore<Id, Event>
where
    Id: Eq,
{
    type SourceId = Id;
    type Event = Event;
    type Error = EmptyStoreError;

    fn append(
        &mut self,
        source_id: Self::SourceId,
        version: Expected,
        events: Vec<Self::Event>,
    ) -> BoxFuture<Result<u32>> {
        Box::pin(async { Err(EmptyStoreError::NotPermitted) })
    }

    fn stream(
        &self,
        source_id: Self::SourceId,
        select: Select,
    ) -> BoxFuture<Result<EventStream<Self>>> {
        Box::pin(async { Err(EmptyStoreError::NotPermitted) })
    }

    fn stream_all(&self, select: Select) -> BoxFuture<Result<EventStream<Self>>> {
        Box::pin(async { Err(EmptyStoreError::NotPermitted) })
    }

    fn remove(&mut self, source_id: Self::SourceId) -> BoxFuture<Result<()>> {
        Box::pin(async { Err(EmptyStoreError::NotPermitted) })
    }
}
