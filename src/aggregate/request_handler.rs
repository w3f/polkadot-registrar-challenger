use crate::state::IdentityAddress;
use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;
use std::marker::PhantomData;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "rpc_requests")]
pub struct RequestHandlerId;

pub enum RequestHandlerCommand {
    RequestState(IdentityAddress),
}

pub struct RequestHandlerAggregate {}

impl Aggregate for RequestHandlerAggregate {
    type Id = RequestHandlerId;
    type State = ();
    type Event = ();
    type Command = RequestHandlerCommand;
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
