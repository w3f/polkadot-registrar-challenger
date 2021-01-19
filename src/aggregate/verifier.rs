use crate::event::{Event, ExternalMessage, IdentityVerification};
use crate::projection::{
    IdentityField, IdentityStateLock, VerificationOutcome, VerificationStatus,
};
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
    async fn handle_verify_message(
        state: &IdentityStateLock<'a>,
        event: Event<ExternalMessage>,
    ) -> Result<Option<Vec<Event<IdentityVerification>>>> {
        let body = event.body();
        let (identity_field, provided_message) = (
            IdentityField::from((body.origin, body.field_address)),
            body.message,
        );

        // Verify message by acquiring a *reader*
        let verification_outcomes = {
            let reader = state.read().await;
            reader.verify_message(&identity_field, &provided_message)
        };

        // If corresponding identities have been found, generate events, acquire
        // a *writer* and update the state if necessary.
        if !verification_outcomes.is_empty() {
            let mut events = vec![];
            let mut writer = state.write().await;

            for outcome in verification_outcomes {
                let address = outcome.address;

                events.push(match outcome.status {
                    VerificationStatus::Valid => {
                        // TODO: Log `false`?
                        writer.set_verified(&address, &identity_field);
                        // TODO: Handle unwrap
                        let is_fully_verified = writer.is_fully_verified(&address).unwrap();

                        IdentityVerification {
                            address: address,
                            field: identity_field.clone(),
                            provided_message: provided_message.clone(),
                            expected_message: outcome.expected_message,
                            is_valid: true,
                            is_fully_verified: is_fully_verified,
                        }
                    }
                    VerificationStatus::Invalid => IdentityVerification {
                        address: address,
                        field: identity_field.clone(),
                        provided_message: provided_message.clone(),
                        expected_message: outcome.expected_message,
                        is_valid: false,
                        is_fully_verified: false,
                    },
                });
            }
        } else {
            return Ok(None);
        }

        Ok(None)
    }
}

impl<'is> Aggregate for VerifierAggregate<'is> {
    type Id = VerifierAggregateId;
    type State = IdentityStateLock<'is>;
    type Event = Event<IdentityVerification>;
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
