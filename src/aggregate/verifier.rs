use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;

pub struct VerifierAggregate {}

impl Aggregate for VerifierAggregate {
    type Id = ();
    type State = ();
    type Event = ();
    type Command = ();
    type Error = failure::Error;

    fn apply(state: Self::State, event: Self::Event) -> Result<Self::State> {
        unimplemented!()
    }

    fn handle<'a, 's>(
        &'a self,
        id: &'s Self::Id,
        state: &'s Self::State,
        command: Self::Command,
    ) -> BoxFuture<Result<Option<Vec<Self::Event>>>>
    where
        's: 'a,
    {
        unimplemented!()
    }
}
