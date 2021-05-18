use crate::actors::Verifier;
use crate::database::Database;
use crate::primitives::ExternalMessage;
use crate::{AccountsConfig, Result};
use actix::prelude::*;
use tokio::time::{interval, Duration};

pub mod email;
pub mod matrix;
pub mod twitter;

pub async fn start_adapters(config: AccountsConfig, db: Database) -> Result<()> {
    let listener = AdapterListener::new(db).await;

    // Matrix client configuration and execution.
    if config.matrix.enabled {
        let config = config.matrix;

        let matrix_client = matrix::MatrixClient::new(
            &config.homeserver,
            &config.username,
            &config.password,
            &config.db_path,
        )
        .await?;

        listener
            .start_message_adapter(matrix_client, config.request_interval)
            .await;
    }

    // Twitter client configuration and execution.
    if config.twitter.enabled {
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
    }

    // Email client configuration and execution.
    if config.email.enabled {
        let config = config.email;

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
    }

    Ok(())
}

#[async_trait]
pub trait Adapter {
    fn name(&self) -> &'static str;
    async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>>;
}

pub struct AdapterListener {
    verifier: Addr<Verifier>,
}

impl AdapterListener {
    pub async fn new(db: Database) -> Self {
        AdapterListener {
            verifier: Verifier::new(db).start(),
        }
    }
    pub async fn start_message_adapter<T>(&self, mut adapter: T, timeout: u64)
    where
        T: 'static + Adapter + Send,
    {
        let mut interval = interval(Duration::from_secs(timeout));

        let verifier = self.verifier.clone();
        tokio::spawn(async move {
            loop {
                // Timeout (skipped the first time);
                interval.tick().await;

                // Fetch message and send it to the listener, if any.
                match adapter.fetch_messages().await {
                    Ok(messages) => {
                        for message in messages {
                            debug!("Received message: {:?}", message);
                            verifier.do_send(message);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    pub struct MessageInjector {
        messages: Arc<Mutex<Vec<ExternalMessage>>>,
    }

    impl MessageInjector {
        pub async fn send(&self, msg: ExternalMessage) {
            let mut lock = self.messages.lock().await;
            (*lock).push(msg);
        }
    }

    #[async_trait]
    impl Adapter for MessageInjector {
        fn name(&self) -> &'static str {
            "test_state_injector"
        }
        async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>> {
            let mut lock = self.messages.lock().await;
            Ok(std::mem::take(&mut *lock))
        }
    }
}
