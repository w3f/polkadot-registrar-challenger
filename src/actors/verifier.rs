use crate::actors::api::NotifyAccountState;
use crate::database::{Database, VerificationOutcome};
use crate::primitives::ExternalMessage;
use crate::primitives::{JudgementState, NotificationMessage};
use crate::Result;
use actix::prelude::*;
use actix_broker::{Broker, SystemBroker};
use tokio::time::{interval, Duration};

pub struct Verifier {
    db: Database,
}

impl Verifier {
    pub fn new(db: Database) -> Self {
        Verifier { db: db }
    }
}

impl Actor for Verifier {
    type Context = Context<Self>;
}

impl Handler<ExternalMessage> for Verifier {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: ExternalMessage, ctx: &mut Self::Context) -> Self::Result {
        fn notify_session(state: JudgementState, notifications: Vec<NotificationMessage>) {
            Broker::<SystemBroker>::issue_async(NotifyAccountState {
                state: state,
                notifications: notifications,
            });
        }

        let db = self.db.clone();
        Box::pin(
            async move {
                debug!("Verifying message: {:?}", msg);
                let outcome = match db.verify_message(&msg).await {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        error!("Failed to verify message: {:?}", err);
                        return ();
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
            }
            .into_actor(self),
        )
    }
}
