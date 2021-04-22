use super::{Aggregate, Snapshot};
use crate::event::{Event, ExternalMessage};
use crate::Result;
use std::convert::AsRef;
use std::convert::TryFrom;

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
pub struct MessageWatcher;

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

    async fn apply(&mut self, _event: Self::Event) -> Result<()> {
        Ok(())
    }

    async fn handle(&self, command: Self::Command) -> Result<Option<Vec<Self::Event>>> {
        match command {
            MessageWatcherCommand::AddMessage(message) => Ok(Some(vec![Event::from(message)])),
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
