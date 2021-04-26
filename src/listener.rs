use crate::database::Database;
use crate::primitives::ExternalMessage;
use crate::{EmailConfig, MatrixConfig, Result, TwitterConfig};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub struct AdapterListener {
    db: Database,
}

impl AdapterListener {
    pub async fn start(&self) -> Result<()> {
        let (tx, mut rx): (
            UnboundedSender<ExternalMessage>,
            UnboundedReceiver<ExternalMessage>,
        ) = unbounded_channel();

        let t_tx = tx.clone();
        tokio::spawn(async move {
            let _ = t_tx;
        });

        // Listen for received messages from external sources, and verify those.
        while let Some(msg) = rx.recv().await {
            self.db.verify_message(msg).await?;
        }

        error!("Event loop for external messages ended unexpectedly");

        Ok(())
    }
}
