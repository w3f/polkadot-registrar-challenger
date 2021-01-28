use crate::event::{Event, ExternalMessage};
use crate::state::{
    IdentityAddress, IdentityField, IdentityInfo, IdentityState, Validity, VerificationOutcome,
    VerificationStatus,
};
use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;
use std::marker::PhantomData;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "aggregate_verifier_id")]
pub struct VerifierAggregateId;

pub enum VerifierCommand {
    VerifyMessage(Event),
}

pub struct VerifierAggregate<'a> {
    _p: PhantomData<&'a ()>,
}

impl<'a> VerifierAggregate<'a> {
    fn handle_verify_message(
        state: &IdentityState<'a>,
        event: Event,
    ) -> Result<Option<Vec<Event>>> {
        let body = event.expect_external_message()?;
        let (identity_field, provided_message) = (
            IdentityField::from((body.origin, body.field_address)),
            body.message,
        );

        // Verify message by acquiring a reader to the state.
        let verification_outcomes = state.verify_message(&identity_field, &provided_message);

        // If corresponding identities have been found, generate the
        // corresponding events.
        let mut events = vec![];
        for outcome in verification_outcomes {
            let net_address = outcome.net_address;
            let mut state = state.lookup_full_state(&net_address).unwrap();

            match outcome.status {
                VerificationStatus::Valid => {
                    state.set_validity(&identity_field, Validity::Valid)?
                }
                VerificationStatus::Invalid => {
                    state.set_validity(&identity_field, Validity::Invalid)?
                }
            };

            events.push(state.into());
        }

        if events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(events))
        }
    }
    fn apply_state_changes(state: &mut IdentityState<'a>, event: Event) {
        /*
        let body = event.body();
        let net_address = body.net_address;
        let field = body.field;

        if body.is_valid {
            // TODO: Handle `false`?
            state.set_verified(&net_address, &field);
        }
        */
    }
}

impl<'is> Aggregate for VerifierAggregate<'is> {
    type Id = VerifierAggregateId;
    type State = IdentityState<'is>;
    type Event = Event;
    // This aggregate has a single purpose. No commands required.
    type Command = VerifierCommand;
    type Error = anyhow::Error;

    fn apply(mut state: Self::State, event: Self::Event) -> Result<Self::State> {
        Self::apply_state_changes(&mut state, event);
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
                VerifierCommand::VerifyMessage(event) => Self::handle_verify_message(state, event),
            }
        };

        Box::pin(fut)
    }
}
