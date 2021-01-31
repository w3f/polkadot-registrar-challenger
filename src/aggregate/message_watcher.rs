use crate::event::{Event, ExternalMessage};
use crate::Result;
use eventually::Aggregate;
use futures::future::BoxFuture;

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct MessageWatcherId;

pub enum MessageWatcherCommand {
    AddMessage(ExternalMessage),
}

pub struct MessageWatcher {}

impl Aggregate for MessageWatcher {
    type Id = MessageWatcherId;
    type State = ();
    type Event = Event;
    type Command = MessageWatcherCommand;
    type Error = anyhow::Error;

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
                MessageWatcherCommand::AddMessage(message) => Ok(Some(vec![message.into()])),
            }
        };

        Box::pin(fut)
    }
}
