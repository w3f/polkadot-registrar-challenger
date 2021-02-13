use super::Aggregate;
use crate::event::{
    DisplayNamePersisted, Event, EventType, ExternalMessage, FieldStatusVerified,
    IdentityFullyVerified, IdentityInserted,
};
use crate::manager::{DisplayName, IdentityField, IdentityManager, IdentityState, NetworkAddress};
use crate::Result;
use futures::future::BoxFuture;
use std::convert::{TryFrom, TryInto};

#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
pub struct VerifierAggregateId;
#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
pub struct VerifierAggregateSnapshots;

impl AsRef<str> for VerifierAggregateId {
    fn as_ref(&self) -> &str {
        "identity_state_changes"
    }
}

impl AsRef<str> for VerifierAggregateSnapshots {
    fn as_ref(&self) -> &str {
        "identity_state_changes_snapshots"
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum VerifierCommand {
    InsertIdentity(IdentityState),
    VerifyMessage(ExternalMessage),
    VerifyDisplayName {
        net_address: NetworkAddress,
        display_name: DisplayName,
    },
    PersistDisplayName {
        net_address: NetworkAddress,
        display_name: DisplayName,
    },
}

#[derive(Debug, Clone, Default)]
pub struct VerifierAggregate {
    state: IdentityManager,
}

impl VerifierAggregate {
    fn handle_verify_message(
        &self,
        external_message: ExternalMessage,
    ) -> Result<Option<Vec<Event>>> {
        let (identity_field, provided_message) = (
            IdentityField::from((external_message.origin, external_message.field_address)),
            external_message.message,
        );

        let mut events: Vec<Event> = vec![];

        // Verify the message.
        let mut c_net_address = None;
        self.state
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
            self.state.is_fully_verified(&net_address).map(|it_is| {
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
        &self,
        net_address: NetworkAddress,
        display_name: DisplayName,
    ) -> Result<Option<Vec<Event>>> {
        let mut events: Vec<Event> = vec![];

        // Verify the display name.
        let mut c_net_address = None;
        self.state
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
            self.state.is_fully_verified(&net_address).map(|it_is| {
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
    fn apply_state_changes(&mut self, event: Event) -> Result<()> {
        match event.body {
            EventType::IdentityInserted(identity) => {
                self.state.insert_identity(identity);
            }
            EventType::FieldStatusVerified(field_status_verified) => {
                self.state.update_field(field_status_verified)?;
            }
            EventType::IdentityFullyVerified(_) => {}
            EventType::DisplayNamePersisted(persisted) => {
                self.state.persist_display_name(persisted)?;
            }
            _ => warn!("Received unrecognized event type when applying changes"),
        }

        Ok(())
    }
}

#[async_trait]
impl Aggregate for VerifierAggregate {
    type Id = VerifierAggregateId;
    type Event = Event;
    type State = IdentityManager;
    type Command = VerifierCommand;
    type Error = anyhow::Error;

    fn state(&self) -> &Self::State {
        &self.state
    }

    async fn apply(&mut self, event: Self::Event) -> Result<()> {
        self.apply_state_changes(event)
    }

    async fn handle(&self, command: Self::Command) -> Result<Option<Vec<Self::Event>>> {
        match command {
            VerifierCommand::InsertIdentity(identity) => {
                if !self.state.contains(&identity) {
                    Ok(Some(vec![Event::from(IdentityInserted {
                        identity: identity,
                    })]))
                } else {
                    Ok(None)
                }
            }
            VerifierCommand::VerifyMessage(message) => self.handle_verify_message(message),
            VerifierCommand::VerifyDisplayName {
                net_address,
                display_name,
            } => self.handle_display_name(net_address, display_name),
            VerifierCommand::PersistDisplayName {
                net_address,
                display_name,
            } => Ok(Some(vec![Event::from(DisplayNamePersisted {
                net_address: net_address,
                display_name: display_name,
            })])),
        }
    }
}
