use super::{Aggregate, Snapshot};
use crate::adapters::email::EmailId;
use crate::event::{Event, EventType, ExternalMessage, ExternalOrigin};
use crate::Result;
use std::collections::HashSet;
use std::convert::{AsRef, TryFrom};

#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
pub struct MessageWatcherId;

// TODO: Required?
impl TryFrom<String> for MessageWatcherId {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        if value == "external_messages" {
            Ok(MessageWatcherId)
        } else {
            Err(anyhow!("Invalid aggregate Id, expected: 'external_messages'").into())
        }
    }
}

impl AsRef<str> for MessageWatcherId {
    fn as_ref(&self) -> &str {
        "external_messages"
    }
}

#[derive(Debug, Clone)]
pub enum MessageWatcherCommand {
    AddMessage(ExternalMessage),
}

/// This is a simple aggregate which adds messages from external sources into
/// the event store. No state must be maintained. The message themselves are
/// verified by the `VerifiedAggregate`.
#[derive(Debug, Clone)]
pub struct MessageWatcher {
    email_tracker: HashSet<EmailId>,
    matrix_tracker: HashSet<()>,
    twitter_tracker: HashSet<()>,
}

#[async_trait]
impl Aggregate for MessageWatcher {
    type Id = MessageWatcherId;
    type State = ();
    type Event = Event;
    type Command = MessageWatcherCommand;
    type Error = anyhow::Error;

    #[cfg(test)]
    fn state(&self) -> &Self::State {
        &()
    }

    // TODO: Track messages.
    async fn apply(&mut self, event: Self::Event) -> Result<()> {
        match event.body {
            EventType::ExternalMessage(message) => {
                match message.origin {
                    ExternalOrigin::Email(id) => {
                        self.email_tracker.insert(id);
                    }
                    // Matrix does not have a ID to track. This part is handled
                    // by the Matrix SDK (which syncs a tracking token).
                    ExternalOrigin::Matrix => {}
                    ExternalOrigin::Twitter(_) => {
                        // TODO
                    }
                }
            }
            _ => {
                return Err(anyhow!(
                    "received invalid type when applying event in MessageWatcher. This is a bug"
                ))
            }
        }

        Ok(())
    }

    // TODO: Filter out existing messages.
    async fn handle(&self, command: Self::Command) -> Result<Option<Vec<Self::Event>>> {
        match command {
            MessageWatcherCommand::AddMessage(message) => {
                // Only create events for messages which are not tracked
                match message.origin {
                    ExternalOrigin::Email(id) => {
                        if self.email_tracker.contains(&id) {
                            return Ok(None);
                        }
                    }
                    // Matrix does not have a ID to track. This part is handled
                    // by the Matrix SDK (which syncs a tracking token).
                    ExternalOrigin::Matrix => {}
                    ExternalOrigin::Twitter(_) => {
                        // TODO
                    }
                }

                Ok(Some(vec![Event::from(message)]))
            }
        }
    }
}

// Since no state is maintained, no snapshot implementation is required.
#[async_trait]
impl Snapshot for MessageWatcher {
    type Id = MessageWatcherId;
    type State = Event;
    type Error = anyhow::Error;

    fn qualifies(&self) -> bool {
        unimplemented!()
    }
    async fn snapshot(&self) -> Self::State {
        unimplemented!()
    }
    async fn restore(state: Self::State) -> Result<Self> {
        unimplemented!()
    }
}
