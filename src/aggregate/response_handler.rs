use crate::event::{Event, FullStateNotFoundResponse};
use crate::state::{IdentityInfo, IdentityState, NetworkAddress};
use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;
use std::marker::PhantomData;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "rpc_responses")]
pub struct ResponseHandlerId;

pub enum ResponseHandlerCommand {
    RequestState(NetworkAddress),
}

pub struct ResponseHandlerAggregate<'a> {
    _p1: PhantomData<&'a ()>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum EventType {
    IdentityInfo(Event<IdentityInfo>),
    FullStateNotFoundResponse(Event<FullStateNotFoundResponse>),
}

impl From<IdentityInfo> for EventType {
    fn from(val: IdentityInfo) -> Self {
        EventType::IdentityInfo(val.into())
    }
}

impl<'is> Aggregate for ResponseHandlerAggregate<'is> {
    type Id = ResponseHandlerId;
    type State = IdentityState<'is>;
    type Event = EventType;
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
        let fut = async move {
            match command {
                ResponseHandlerCommand::RequestState(net_address) => {
                    let event: EventType = state
                        .lookup_full_state(&net_address)
                        .map(|info| info.into())
                        .or_else(|| {
                            Some(EventType::FullStateNotFoundResponse(
                                FullStateNotFoundResponse {
                                    net_address: net_address,
                                }
                                .into(),
                            ))
                        })
                        // Unwrapping here is fine, since this will always return `Some(..)`
                        .unwrap();

                    Result::<Option<Vec<Self::Event>>>::Ok(Some(vec![event]))
                }
            }
        };

        Box::pin(fut);

        unimplemented!()
    }
}
