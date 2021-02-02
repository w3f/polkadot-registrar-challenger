use super::display_name::DisplayNameHandler;
use super::Error;
use crate::event::{Event, EventType, ExternalMessage, FieldStatusVerified, IdentityFullyVerified};
use crate::manager::{
    DisplayName, FieldStatus, IdentityAddress, IdentityField, IdentityManager, IdentityState,
    NetworkAddress, Validity, VerificationOutcome,
};
use eventually::Aggregate;
use futures::future::BoxFuture;
use std::convert::{TryFrom, TryInto};
use std::marker::PhantomData;

type Result<T> = std::result::Result<T, Error>;

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct VerifierAggregateId;

impl TryFrom<String> for VerifierAggregateId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        if value == "field_verifications" {
            Ok(VerifierAggregateId)
        } else {
            Err(anyhow!("Invalid aggregate Id, expected: 'field_verifications'").into())
        }
    }
}

impl AsRef<str> for VerifierAggregateId {
    fn as_ref(&self) -> &str {
        "field_verifications"
    }
}

pub enum VerifierCommand {
    VerifyMessage(ExternalMessage),
    VerifyDisplayName(DisplayName),
    PersistDisplayName(DisplayName),
}

pub struct VerifierAggregate<'a> {
    _p: PhantomData<&'a ()>,
}

impl<'a> VerifierAggregate<'a> {
    fn handle_verify_message(
        state: &IdentityManager<'a>,
        external_message: ExternalMessage,
    ) -> Result<Option<Vec<Event>>> {
        let (identity_field, external_message) = (
            IdentityField::from((external_message.origin, external_message.field_address)),
            external_message.message,
        );

        let mut events: Vec<Event> = vec![];

        // Verify the message.
        let mut c_net_address = None;
        state
            .verify_message(&identity_field, &external_message)
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
    fn handle_display_name(
        state: &IdentityManager<'a>,
        net_address: NetworkAddress,
        display_name: DisplayName,
    ) -> Result<Option<Vec<Event>>> {
        let handler = DisplayNameHandler::with_state(state.get_display_names());
        let violations = handler.verify_display_name(&display_name);

        Ok(None)
    }
    fn apply_state_changes(state: &mut IdentityManager<'a>, event: Event) {
        match event.body {
            EventType::FieldStatusVerified(field_status_verified) => {
                let _ = state.update_field(field_status_verified).map_err(|err| {
                    error!("{}", err);
                });
            }
            EventType::IdentityFullyVerified(_) => {}
            _ => warn!("Received unrecognized event type when applying changes"),
        }
    }
}

impl<'is> Aggregate for VerifierAggregate<'is> {
    type Id = VerifierAggregateId;
    type State = IdentityManager<'is>;
    type Event = Event;
    type Command = VerifierCommand;
    type Error = Error;

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
                VerifierCommand::VerifyMessage(external_message) => {
                    Self::handle_verify_message(state, external_message)
                }
                VerifierCommand::VerifyDisplayName(display_name) => {
                    //Self::handle_display_name(state, display_name)
                    unimplemented!()
                }
                // TODO
                VerifierCommand::PersistDisplayName(_) => unimplemented!(),
            }
        };

        Box::pin(fut)
    }
}
