use crate::Result;
use crate::event::{Event};
use eventually::Aggregate;
use futures::future::BoxFuture;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "rpc_responses")]
pub struct ResponseHandlerId;

pub enum ResponseHandlerCommand {

}

pub struct ResponseHandlerAggregate {}

impl Aggregate for ResponseHandlerAggregate {
    type Id = ResponseHandlerId;
    type State = ();
    type Event = Event<()>;
    type Command = ResponseHandlerCommand;
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
