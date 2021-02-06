use super::Error;
use crate::event::{
    Event, EventType, ExternalMessage, FieldStatusVerified, IdentityFullyVerified, IdentityInserted,
};
use crate::manager::{
    DisplayName, FieldStatus, IdentityAddress, IdentityField, IdentityManager, IdentityState,
    NetworkAddress, UpdateChanges, Validity, VerificationOutcome,
};
use async_channel::Sender;
use eventually::Aggregate;
use eventually_event_store_db::GenericEvent;
use futures::future::BoxFuture;
use std::convert::{TryFrom, TryInto};

type Result<T> = std::result::Result<T, Error>;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
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

#[derive(Debug, Clone)]
pub enum VerifierCommand {
    InsertIdentity(IdentityState),
    VerifyMessage {
        message: ExternalMessage,
        callback: Option<Sender<UpdateChanges>>,
    },
    VerifyDisplayName {
        net_address: NetworkAddress,
        display_name: DisplayName,
    },
    PersistDisplayName {
        net_address: NetworkAddress,
        display_name: DisplayName,
    },
}

#[derive(Debug, Clone)]
pub struct VerifierAggregate;

impl VerifierAggregate {
    fn handle_verify_message(
        state: &IdentityManager,
        external_message: ExternalMessage,
    ) -> Result<Option<Vec<GenericEvent>>> {
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
            Ok(Some(
                events
                    .into_iter()
                    .map(|event| event.try_into().map_err(|err| Error::from(err)))
                    .collect::<Result<Vec<GenericEvent>>>()?,
            ))
        }
    }
    fn handle_display_name(
        state: &IdentityManager,
        net_address: NetworkAddress,
        display_name: DisplayName,
    ) -> Result<Option<Vec<GenericEvent>>> {
        let mut events: Vec<Event> = vec![];

        // Verify the display name.
        let mut c_net_address = None;
        state
            .verify_display_name(net_address, display_name)?
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
            Ok(Some(
                events
                    .into_iter()
                    .map(|event| event.try_into().map_err(|err| Error::from(err)))
                    .collect::<Result<Vec<GenericEvent>>>()?,
            ))
        }
    }
    fn apply_state_changes(state: &mut IdentityManager, event: GenericEvent) {
        let event: Event = if let Ok(event) = event.as_json() {
            event
        } else {
            // This should never occur, since the `handle` method creates the events.
            error!("Failed to apply changes, event could not be deserialized");
            return;
        };

        match event.body {
            EventType::FieldStatusVerified(field_status_verified) => {
                let _ = state.update_field(field_status_verified).map_err(|err| {
                    error!("{}", err);
                });
            }
            EventType::IdentityInserted(identity) => {
                state.insert_identity(identity);
            }
            EventType::IdentityFullyVerified(_) => {}
            _ => warn!("Received unrecognized event type when applying changes"),
        }
    }
}

impl Aggregate for VerifierAggregate {
    type Id = VerifierAggregateId;
    type State = IdentityManager;
    type Event = GenericEvent;
    type Command = VerifierCommand;
    type Error = Error;

    fn apply(mut state: Self::State, event: Self::Event) -> Result<Self::State> {
        Self::apply_state_changes(&mut state, event);
        Ok(state)
    }

    fn handle<'a, 's>(
        &'a self,
        _id: &'s Self::Id,
        state: &'s Self::State,
        command: Self::Command,
    ) -> BoxFuture<Result<Option<Vec<Self::Event>>>>
    where
        's: 'a,
    {
        let fut = async move {
            match command {
                VerifierCommand::InsertIdentity(identity) => {
                    Ok(Some(vec![Event::from(IdentityInserted {
                        identity: identity,
                    })
                    .try_into()?]))
                }
                VerifierCommand::VerifyMessage { message, callback } => {
                    Self::handle_verify_message(state, message)
                }
                VerifierCommand::VerifyDisplayName {
                    net_address,
                    display_name,
                } => Self::handle_display_name(state, net_address, display_name),
                // TODO
                VerifierCommand::PersistDisplayName {
                    net_address: _,
                    display_name: _,
                } => unimplemented!(),
            }
        };

        Box::pin(fut)
    }
}
