use super::Projection;
use crate::aggregate::verifier::VerifierAggregateId;
use crate::aggregate::Error;
use crate::api::ConnectionPool;
use crate::event::{Event, EventType};

#[derive(Default)]
pub struct SessionNotifier {
    connection_pool: ConnectionPool,
}

impl SessionNotifier {
    pub fn with_pool(pool: ConnectionPool) -> Self {
        SessionNotifier {
            connection_pool: pool,
        }
    }
}

#[async_trait]
impl Projection for SessionNotifier {
    type Id = VerifierAggregateId;
    type Event = Event;
    type Error = Error;

    async fn project(&mut self, event: Self::Event) -> Result<(), Error> {
        let net_address = match event.body {
            EventType::FieldStatusVerified(ref field_status) => &field_status.net_address,
            EventType::IdentityFullyVerified(ref verified) => &verified.net_address,
            _ => return Ok(()),
        };

        if let Some(notifier) = self.connection_pool.notify_net_address(net_address) {
            if let Err(_) = notifier.send(event).await {
                info!("All connections to RPC sessions are closed");
            }
        }

        Ok(())
    }
}
