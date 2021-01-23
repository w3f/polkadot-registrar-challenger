use crate::event::{Event, ExternalMessage, IdentityVerification};
use crate::projection::{IdentityField, IdentityState, VerificationOutcome, VerificationStatus};
use crate::Result;
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
    _p: PhantomData<&'a ()>,
}

impl<'a> VerifierAggregate<'a> {
    fn handle_verify_message(
        state: &IdentityState<'a>,
        event: Event<ExternalMessage>,
    ) -> Result<Option<Vec<Event<IdentityVerification>>>> {
        let body = event.body();
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
            let address = outcome.address;

            events.push(match outcome.status {
                VerificationStatus::Valid => {
                    // TODO: Handle unwrap
                    let is_fully_verified = state.is_fully_verified(&address).unwrap();

                    IdentityVerification {
                        address: address.clone(),
                        field: identity_field.clone(),
                        provided_message: provided_message.clone(),
                        expected_message: outcome.expected_message.clone(),
                        is_valid: true,
                        is_fully_verified: is_fully_verified,
                    }
                    .into()
                }
                VerificationStatus::Invalid => IdentityVerification {
                    address: address.clone(),
                    field: identity_field.clone(),
                    provided_message: provided_message.clone(),
                    expected_message: outcome.expected_message.clone(),
                    is_valid: false,
                    is_fully_verified: false,
                }
                .into(),
            });
        }

        if events.is_empty() {
            Ok(None)
        } else {
            Ok(Some(events))
        }
    }
    fn apply_state_changes(state: &mut IdentityState<'a>, event: Event<IdentityVerification>) {
        let body = event.body();
        let address = body.address;
        let field = body.field;

        if body.is_valid {
            // TODO: Handle `false`?
            state.set_verified(&address, &field);

            // Check whether `is_valid` and `is_fully_verified` are both `true`, return error.
            if body.is_fully_verified {
                // TODO: Notify Websocket responder?
                state.remove_identity(&address);
            }
        }
    }
}

impl<'is> Aggregate for VerifierAggregate<'is> {
    type Id = VerifierAggregateId;
    type State = IdentityState<'is>;
    type Event = Event<IdentityVerification>;
    // This aggregate has a single purpose. No commands required.
    type Command = VerifierCommand;
    type Error = failure::Error;

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
