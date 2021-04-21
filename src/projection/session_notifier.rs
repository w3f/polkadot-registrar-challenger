use super::Projection;
use crate::api::ConnectionPool;
use crate::api_v2::lookup_server::AddIdentityState;
use crate::event::{Event, EventType, Notification, StateWrapper};
use crate::Result;
use crate::{aggregate::verifier::VerifierAggregateId, manager::IdentityManager};
use actix::prelude::*;
use actix_broker::BrokerIssue;
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

// Required to issue messages to the broker.
impl Actor for SessionNotifier {
    type Context = Context<Self>;
}

#[async_trait]
impl Projection for SessionNotifier {
    type Id = VerifierAggregateId;
    type Event = Event;
    type Error = anyhow::Error;

    async fn project(&mut self, event: Self::Event) -> Result<()> {
        // Process actor message.
        let add_identity = match event.body {
            EventType::IdentityInserted(inserted) => AddIdentityState::IdentityInserted(inserted),
            EventType::FieldStatusVerified(verified) => {
                AddIdentityState::FieldStatusVerified(verified)
            }
            _ => return Ok(()),
        };

        // Send identity to the websocket broker.
        self.issue_system_async::<AddIdentityState>(add_identity);

        Ok(())
    }
}
