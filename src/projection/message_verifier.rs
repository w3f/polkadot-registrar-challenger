use super::Projection;
use crate::aggregate::message_watcher::MessageWatcherId;
use crate::aggregate::verifier::{VerifierAggregate, VerifierCommand};
use crate::aggregate::{Error, Repository};
use crate::event::{Event, EventType};

pub struct MessageVerifier {
    repository: Repository<VerifierAggregate>,
}

#[async_trait]
impl Projection for MessageVerifier {
    type Id = MessageWatcherId;
    type Event = Event;
    type Error = Error;

    async fn project(&mut self, event: Self::Event) -> Result<(), Error> {
        let message = match event.body {
            EventType::ExternalMessage(message) => message,
            _ => {
                return Err(
                    anyhow!("Received unexpected message in MessageVerifier projection").into(),
                )
            }
        };

        self.repository
            .apply(VerifierCommand::VerifyMessage(message))
            .await?;

        Ok(())
    }
}
