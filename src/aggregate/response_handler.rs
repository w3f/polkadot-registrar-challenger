use crate::api::SubId;
use crate::event::{ErrorMessage, Event, EventType};
use crate::manager::{IdentityManager, IdentityState, NetworkAddress};
use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;
use std::marker::PhantomData;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "rpc_responses")]
pub struct ResponseHandlerId;

pub enum ResponseHandlerCommand {
    RequestState {
        requester: SubId,
        net_address: NetworkAddress,
    },
}

pub struct ResponseHandlerAggregate<'a> {
    _p1: PhantomData<&'a ()>,
}

impl<'is> Aggregate for ResponseHandlerAggregate<'is> {
    type Id = ResponseHandlerId;
    type State = IdentityManager<'is>;
    type Event = Event;
    type Command = ResponseHandlerCommand;
    type Error = anyhow::Error;

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
                ResponseHandlerCommand::RequestState {
                    requester,
                    net_address,
                } => {
                    let event: Event = state
                        .lookup_full_state(&net_address)
                        .map(|info| info.into())
                        .or_else(|| {
                            Some(
                                ErrorMessage {
                                    requester: Some(requester),
                                    message: format!(
                                        "Network address {} could not be found",
                                        net_address.net_address_str()
                                    ),
                                }
                                .into(),
                            )
                        })
                        // This always returns `Some(..)`.
                        .unwrap();

                    Result::<Option<Vec<Self::Event>>>::Ok(Some(vec![event]))
                }
            }
        };

        Box::pin(fut);

        unimplemented!()
    }
}
