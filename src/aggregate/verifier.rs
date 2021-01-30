use crate::event::{Event, ExternalMessage, FieldStatusVerified, IdentityFullyVerified};
use crate::manager::{
    FieldStatus, IdentityAddress, IdentityField, IdentityManager, IdentityState, Validity,
    VerificationOutcome,
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
        state: &IdentityManager<'a>,
        event: Event,
    ) -> Result<Option<Vec<Event>>> {
        let body = event.expect_external_message()?;
        let (identity_field, provided_message) = (
            IdentityField::from((body.origin, body.field_address)),
            body.message,
        );

        let mut events: Vec<Event> = vec![];

        // Verify the message.
        let mut c_net_address = None;
        state
            .verify_message(&identity_field, &provided_message)
            .map(|outcome| {
                c_net_address = Some(outcome.net_address.clone());

                events.push(
                    FieldStatusVerified {
                        net_address: outcome.net_address,
                        field_status: outcome.field_status,
                    }
                    .into(),
                );
            });

        // If a message has been successfully verified (and `c_net_address` is
        // therefore `Some(..)`), then check whether the full identity has been
        // verified and create an event if that's the case.
        if let Some(net_address) = c_net_address {
            state.is_fully_verified(&net_address).map(|it_is| {
                if it_is {
                    events.push(
                        IdentityFullyVerified {
                            net_address: net_address,
                        }
                        .into(),
                    );
                }
            })?;
        }

        if events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(events))
        }
    }
    fn apply_state_changes(state: &mut IdentityManager<'a>, event: Event) {
        /*
        let event = event.exp
        let body = event.body;
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
    type State = IdentityManager<'is>;
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
