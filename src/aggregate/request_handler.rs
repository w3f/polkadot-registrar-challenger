use crate::api::SubId;
use crate::event::{Event, FullStateRequest};
use crate::state::NetworkAddress;
use eventually::Aggregate;
use futures::future::BoxFuture;

type Result<T> = std::result::Result<T, crate::Error>;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "rpc_requests")]
pub struct RequestHandlerId;

#[derive(Debug, Clone)]
pub enum RequestHandlerCommand {
    RequestState {
        requester: SubId,
        net_address: NetworkAddress,
    },
}

#[derive(Debug, Clone)]
pub struct RequestHandlerAggregate {}

impl Aggregate for RequestHandlerAggregate {
    type Id = RequestHandlerId;
    type State = ();
    type Event = Event;
    type Command = RequestHandlerCommand;
    //type Error = anyhow::Error;
    type Error = crate::Error;

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
                RequestHandlerCommand::RequestState {
                    requester,
                    net_address,
                } => Ok(Some(vec![FullStateRequest {
                    requester: requester,
                    net_address: net_address,
                }
                .into()])),
            }
        };

        Box::pin(fut)
    }
}
