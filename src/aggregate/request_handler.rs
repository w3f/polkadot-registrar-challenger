use crate::event::{Event, RequestFullState};
use crate::state::IdentityAddress;
use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;

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
    type Event = Event<RequestFullState>;
    type Command = RequestHandlerCommand;
    type Error = failure::Error;

    /// No state changes occur on this aggregate.
    fn apply(state: Self::State, _event: Self::Event) -> Result<Self::State> {
        Ok(state)
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
        let fut = async move {
            match command {
                RequestHandlerCommand::RequestState(net_address) => {
                    Ok(Some(vec![RequestFullState {
                        net_address: net_address,
                    }
                    .into()]))
                }
            }
        };

        Box::pin(fut)
    }
}
