use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;

pub struct MessageWatcher {}

impl Aggregate for MessageWatcher {
    type Id = ();
    type State = ();
    type Event = ();
    type Command = ();
    type Error = failure::Error;

    fn apply(state: Self::State, _event: Self::Event) -> Result<Self::State> {
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
