use crate::actors::api::NotifyAccountState;
use crate::primitives::{JudgementState, NotificationMessage};
use crate::database::{Database, VerificationOutcome};
use crate::primitives::ExternalMessage;
use crate::{EmailConfig, MatrixConfig, Result, TwitterConfig};
use actix_broker::{Broker, SystemBroker};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::{interval, Duration};

pub struct AdapterListener {
    db: Database,
    tx: UnboundedSender<ExternalMessage>,
    rx: UnboundedReceiver<ExternalMessage>,
}

pub trait Adapter {
    fn name(&self) -> &'static str;
    fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>>;
}

impl AdapterListener {
    pub fn new(db: Database) -> Self {
        let (tx, mut rx) = unbounded_channel();

        AdapterListener {
            db: db,
            tx: tx,
            rx: rx,
        }
    }
    pub async fn start_message_adapter<T>(&self, mut adapter: T, timeout: u64)
    where
        T: 'static + Adapter + Send,
    {
        let mut interval = interval(Duration::from_secs(timeout));

        let tx = self.tx.clone();
        tokio::spawn(async move {
            loop {
                // Timeout (skipped the first time);
                interval.tick().await;

                // Fetch message and send it to the listener, if any.
                match adapter.fetch_messages() {
                    Ok(messages) => {
                        for message in messages {
                            debug!("Received message: {:?}", message);
                            // TODO: Is unwrapping fine here?
                            tx.send(message).unwrap();
                        }
                    }
                    Err(err) => {
                        error!(
                            "Error fetching messages in {} adapter: {:?}",
                            adapter.name(),
                            err
                        );
                    }
                }
            }
        });
    }
    pub async fn start_blocking(&mut self) -> Result<()> {
        fn notify_session(state: JudgementState, notifications: Vec<NotificationMessage>) {
                    Broker::<SystemBroker>::issue_async(NotifyAccountState {
                        state: state,
                        notifications: notifications,
                    });
        }

        // Listen for received messages from external sources, and verify those.
        while let Some(message) = self.rx.recv().await {
            debug!("Verifying message: {:?}", message);

            match self.db.verify_message(&message).await? {
                VerificationOutcome::AlreadyVerified => {
                    debug!("The account field has already been verified: {:?}", message)
                    // Don't inform user.
                }
                VerificationOutcome::Valid {
                    state,
                    notifications,
                } => {
                    info!("Message verification succeeded : {:?}", message);
                    notify_session(state, notifications);
                }
                VerificationOutcome::Invalid {
                    state,
                    notifications,
                } => {
                    info!("Message verification failed: {:?}", message);
                    notify_session(state, notifications);
                }
                VerificationOutcome::SecondChallengeExpected {
                    state,
                    notifications,
                } => {
                    info!("Message verification succeeded: {:?}, second verification expected", message);
                    notify_session(state, notifications);

                    // TODO: Notify client
                }
                VerificationOutcome::NotFound => {
                    debug!(
                        "No judgement state could be found based on the external message: {:?}",
                        message
                    );
                }
            }
        }

        error!("Event loop for external messages ended unexpectedly");

        Ok(())
    }
}