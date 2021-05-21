use crate::actors::api::NotifyAccountState;
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
}

impl Verifier {
    pub fn new(db: Database) -> Self {
        Verifier { db: db }
    }
    pub async fn verify(&self, msg: ExternalMessage) -> Result<()> {
        fn notify_session(state: JudgementState, notifications: Vec<NotificationMessage>) {
            Broker::<SystemBroker>::issue_async(NotifyAccountState {
                state: state,
                notifications: notifications,
            });
        }

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
                notify_session(state, notifications);
            }
            VerificationOutcome::Invalid {
                state,
                notifications,
            } => {
                info!("Message verification failed: {:?}", msg);
                notify_session(state, notifications);
            }
            VerificationOutcome::SecondChallengeExpected {
                state,
                notifications,
            } => {
                info!(
                    "Message verification succeeded: {:?}, second verification expected",
                    msg
                );
                notify_session(state, notifications);

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
