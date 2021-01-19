use crate::Result;
use crate::projection::IdentityStateLock;
use crate::event::{Event, ExternalMessage, IdentityVerification};
use eventually::Aggregate;
use futures::future::BoxFuture;
use std::marker::PhantomData;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "verifier_aggregate_id")]
pub struct VerifierAggregateId;

pub enum VerifierCommand {
    VerifyMessage(Event<ExternalMessage>),
}

pub struct VerifierAggregate<'a> {
    _p: PhantomData<&'a ()>
}

impl<'a> VerifierAggregate<'a> {
    async fn handle_verify_message(state: &IdentityStateLock<'a>, event: Event<ExternalMessage>) {
        let reader = state.read().await;
        //let addresses = reader.verify_message()
    }
}

impl<'is> Aggregate for VerifierAggregate<'is> {
    type Id = VerifierAggregateId;
    type State = IdentityStateLock<'is>;
    type Event = Event<ExternalMessage>;
    // This aggregate has a single purpose. No commands required.
    type Command = VerifierCommand;
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
        let fut = match command {
            VerifierCommand::VerifyMessage(event) => Self::handle_verify_message(state, event),
        };

        unimplemented!()
    }
}
