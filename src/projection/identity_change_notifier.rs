use super::Projection;
use crate::aggregate::Error;
use crate::api::ConnectionPool;
use crate::event::{Event, EventType};
use crate::{aggregate::verifier::VerifierAggregateId, manager::IdentityManager};
use parking_lot::RwLock;
use std::sync::Arc;

pub struct SessionNotifier {
    connection_pool: ConnectionPool,
    manager: Arc<RwLock<IdentityManager>>,
}

impl SessionNotifier {
    pub fn new(pool: ConnectionPool, manager: Arc<RwLock<IdentityManager>>) -> Self {
        SessionNotifier {
            connection_pool: pool,
            manager: manager,
        }
    }
}

#[async_trait]
impl Projection for SessionNotifier {
    type Id = VerifierAggregateId;
    type Event = Event;
    type Error = Error;

    // TODO: This should handle the full identity state, and only send notifications to the RPC API.
    async fn project(&mut self, event: Self::Event) -> Result<(), Error> {
        let net_address = match event.body {
            EventType::IdentityInserted(ref inserted) => &inserted.identity.net_address,
            EventType::FieldStatusVerified(ref field_status) => &field_status.net_address,
            // TODO: Does this need any special handling?
            EventType::IdentityFullyVerified(ref verified) => &verified.net_address,
            _ => return Ok(()),
        };

        let mut was_processed = false;
        if let Some(notifier) = self.connection_pool.notify_net_address(net_address) {
            if let Err(_) = notifier.send(event.clone()).await {
                info!("All connections to RPC sessions are closed");
            } else {
                was_processed = true;
            }
        }

        if !was_processed {
            match event.body {
                EventType::IdentityInserted(inserted) => {
                    self.manager.write().insert_identity(inserted);
                }
                EventType::FieldStatusVerified(verified) => {
                    self.manager.write().update_field(verified)?;
                }
                _ => return Ok(()),
            }
        }

        Ok(())
    }
}
