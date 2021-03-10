use super::Projection;
use crate::api::ConnectionPool;
use crate::event::{Event, EventType, Notification, StateWrapper};
use crate::Result;
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
    type Error = anyhow::Error;

    fn latest_revision(&self) -> u64 {
        unimplemented!()
    }
    fn update_revision(&mut self, revision: u64) {
        unimplemented!()
    }
    async fn project(&mut self, event: Self::Event) -> Result<()> {
        // Clone due to partial move.
        let net_address = match event.body {
            EventType::IdentityInserted(ref inserted) => inserted.identity.net_address.clone(),
            EventType::FieldStatusVerified(ref field_status) => field_status.net_address.clone(),
            // TODO: Does this need any special handling?
            EventType::IdentityFullyVerified(ref verified) => verified.net_address.clone(),
            _ => return Ok(()),
        };

        match event.body {
            EventType::IdentityInserted(inserted) => {
                self.manager.write().insert_identity(inserted.clone());
                self.connection_pool.broadcast(
                    &net_address,
                    StateWrapper::newly_inserted_notification(inserted),
                );
            }
            EventType::FieldStatusVerified(verified) => {
                let notifications: Vec<Notification> = {
                    self.manager
                        .write()
                        .update_field(verified)?
                        .map(|changes| vec![changes.into()])
                        .unwrap_or(vec![])
                };

                if let Some(state) = self.manager.read().lookup_full_state(&net_address) {
                    let state = StateWrapper::with_notifications(state, notifications);
                    self.connection_pool.broadcast(&net_address, state);
                }
            }
            _ => return Ok(()),
        }

        Ok(())
    }
}
