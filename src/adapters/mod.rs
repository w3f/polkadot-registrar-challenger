use crate::{Result, AccountsConfig};
use crate::database::Database;
use crate::actors::Verifier;
use crate::primitives::ExternalMessage;
use tokio::time::{interval, Duration};
use actix::prelude::*;

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

        listener.start_message_adapter(matrix_client, config.request_interval).await;
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

        listener.start_message_adapter(twitter_client, config.request_interval).await;
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