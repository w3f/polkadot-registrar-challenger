use super::Error;
use crate::event::{Event, ExternalMessage};
use eventually::Aggregate;
use eventually_event_store_db::GenericEvent;
use futures::future::BoxFuture;
use std::convert::AsRef;
use std::convert::{TryFrom, TryInto};

type Result<T> = std::result::Result<T, Error>;

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct MessageWatcherId;

impl TryFrom<String> for MessageWatcherId {
    type Error = Error;

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

impl Aggregate for MessageWatcher {
    type Id = MessageWatcherId;
    type State = ();
    type Event = GenericEvent;
    type Command = MessageWatcherCommand;
    type Error = Error;

    fn apply(state: Self::State, _event: Self::Event) -> Result<Self::State> {
        Ok(state)
    }

    fn handle<'a, 's>(
        &'a self,
        _id: &'s Self::Id,
        _state: &'s Self::State,
        command: Self::Command,
    ) -> BoxFuture<Result<Option<Vec<Self::Event>>>>
    where
        's: 'a,
    {
        let fut = async move {
            match command {
                MessageWatcherCommand::AddMessage(message) => {
                    Ok(Some(vec![Event::from(message).try_into()?]))
                }
            }
        };

        Box::pin(fut)
    }
}
