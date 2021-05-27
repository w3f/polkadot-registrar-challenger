use crate::actors::api::{LookupServer, NotifyAccountState};
use crate::database::{Database, VerificationOutcome};
use crate::primitives::ExternalMessage;
use crate::primitives::{JudgementState, NotificationMessage};
use crate::Result;
use actix::prelude::*;
use actix_broker::{Broker, SystemBroker};
use tokio::time::{interval, Duration};

#[derive(Clone)]
pub struct Verifier {
    db: Database,
    server: Addr<LookupServer>,
}

impl Verifier {
    pub fn new(db: Database, server: Addr<LookupServer>) -> Self {
        Verifier {
            db: db,
            server: server,
        }
    }
    /// Notifies the websocket session(s) about the state changes.
    pub fn notify_session(&self, state: JudgementState, notifications: Vec<NotificationMessage>) {
        self.server.do_send(NotifyAccountState {
            state: state,
            notifications: notifications,
        });
    }
    pub async fn verify(&self, msg: ExternalMessage) -> Result<()> {
        debug!("Verifying message: {:?}", msg);
        let outcome = match self.db.verify_message(&msg).await {
            Ok(outcome) => outcome,
            Err(err) => {
                error!("Failed to verify message: {:?}", err);
                return Ok(());
            }
        };

        match outcome {
            VerificationOutcome::AlreadyVerified => {
                // Ignore.
                debug!("The account field has already been verified: {:?}", msg)
            }
            VerificationOutcome::Valid {
                state,
                notifications,
            } => {
                info!("Message verification succeeded : {:?}", msg);
                self.notify_session(state, notifications);
            }
            VerificationOutcome::Invalid {
                state,
                notifications,
            } => {
                info!("Message verification failed: {:?}", msg);
                self.notify_session(state, notifications);
            }
            VerificationOutcome::SecondChallengeExpected {
                state,
                notifications,
            } => {
                info!(
                    "Message verification succeeded: {:?}, second verification expected",
                    msg
                );
                self.notify_session(state, notifications);

                // TODO: Notify client
            }
            VerificationOutcome::NotFound => {
                debug!(
                    "No judgement state could be found based on the external message: {:?}",
                    msg
                );
            }
        }

        Ok(())
    }
}
