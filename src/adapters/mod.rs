use crate::database::Database;
use crate::primitives::{
    ExpectedMessage, ExternalMessage, IdentityFieldValue, NotificationMessage,
};
use crate::{AdapterConfig, Result};
use tokio::time::{interval, Duration};

pub mod email;
pub mod matrix;
pub mod twitter;

pub async fn run_adapters(config: AdapterConfig, db: Database) -> Result<()> {
    let listener = AdapterListener::new(db).await;
    // Convenience flat for logging
    let mut started = false;

    // Matrix client configuration and execution.
    if config.matrix.enabled {
        info!("Starting Matrix adapter...");
        let config = config.matrix;

        let matrix_client = matrix::MatrixClient::new(
            &config.homeserver,
            &config.username,
            &config.password,
            &config.db_path,
        )
        .await?;

        listener.start_message_adapter(matrix_client, 1).await;
        started = true;
    }

    // Twitter client configuration and execution.
    if config.twitter.enabled {
        info!("Starting Twitter adapter...");
        let config = config.twitter;

        let twitter_client = twitter::TwitterBuilder::new()
            .consumer_key(config.api_key)
            .consumer_secret(config.api_secret)
            .token(config.token)
            .token_secret(config.token_secret)
            .build()?;

        listener
            .start_message_adapter(twitter_client, config.request_interval)
            .await;

        started = true;
    }

    // Email client configuration and execution.
    if config.email.enabled {
        info!("Starting email adapter...");
        let config = config.email;

        // TODO: Rename struct
        let email_client = email::SmtpImapClientBuilder::new()
            .smtp_server(config.smtp_server)
            .imap_server(config.imap_server)
            .email_inbox(config.inbox)
            .email_user(config.user)
            .email_password(config.password)
            .build()?;

        listener
            .start_message_adapter(email_client, config.request_interval)
            .await;

        started = true;
    }

    if !started {
        warn!("No adapters are enabled");
    }

    Ok(())
}

#[async_trait]
pub trait Adapter {
    type MessageType;

    fn name(&self) -> &'static str;
    async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>>;
    async fn send_message(&mut self, to: &str, content: Self::MessageType) -> Result<()>;
}

// Filler for adapters that do not send messages.
impl From<ExpectedMessage> for () {
    fn from(_: ExpectedMessage) -> Self {
        ()
    }
}

pub struct AdapterListener {
    db: Database,
}

impl AdapterListener {
    pub async fn new(db: Database) -> Self {
        AdapterListener { db: db }
    }
    pub async fn start_message_adapter<T>(&self, mut adapter: T, timeout: u64)
    where
        T: 'static + Adapter + Send,
        <T as Adapter>::MessageType: From<ExpectedMessage>,
    {
        let mut interval = interval(Duration::from_secs(timeout));

        let mut db = self.db.clone();
        actix::spawn(async move {
            loop {
                // Timeout (skipped the first time);
                interval.tick().await;

                // Fetch message and send it to the listener, if any.
                match adapter.fetch_messages().await {
                    Ok(messages) => {
                        for message in messages {
                            debug!("Processing message from: {:?}", message.origin);
                            let _ = db
                                .process_message(&message)
                                .await
                                .map_err(|err| error!("Error when verifying message: {:?}", err));
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

                // Check if a second challenge must be sent to the user directly.
                // TODO: One might consider putting this logic into a separate task
                // with a lower event loop timeout.
                match db.fetch_events().await {
                    Ok(events) => {
                        for event in &events {
                            match event {
                                NotificationMessage::AwaitingSecondChallenge(_, field_value) => {
                                    match field_value {
                                        IdentityFieldValue::Email(to) => {
                                            if adapter.name() == "email" {
                                                // TODO: Handle unwrap.
                                                debug!("Sending second challenge to {}", to);
                                                let challenge = db
                                                    .fetch_second_challenge(field_value)
                                                    .await
                                                    .unwrap();

                                                adapter
                                                    .send_message(to.as_str(), challenge.into())
                                                    .await
                                                    .unwrap();
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // TODO: Unify error handling.
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
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Clone)]
    pub struct MessageInjector {
        messages: Arc<Mutex<Vec<ExternalMessage>>>,
    }

    impl MessageInjector {
        pub fn new() -> Self {
            MessageInjector {
                messages: Arc::new(Mutex::new(vec![])),
            }
        }
        pub async fn send(&self, msg: ExternalMessage) {
            let mut lock = self.messages.lock().await;
            (*lock).push(msg);
        }
    }

    #[async_trait]
    impl Adapter for MessageInjector {
        type MessageType = ();

        fn name(&self) -> &'static str {
            "test_state_injector"
        }
        async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>> {
            let mut lock = self.messages.lock().await;
            Ok(std::mem::take(&mut *lock))
        }
        async fn send_message(&mut self, _to: &str, _content: Self::MessageType) -> Result<()> {
            unimplemented!()
        }
    }
}
