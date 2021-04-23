use crate::primitives::{IdentityContext, ChainAddress, ChainRemark, ExternalMessage, JudgementState, Timestamp, MessageId};
use crate::Result;
use bson::{doc, from_document, to_bson, to_document, Bson, Document};
use mongodb::{options::UpdateOptions, Client, Database as MongoDb};
use serde::Serialize;
use futures::StreamExt;
use std::collections::HashSet;

const MESSAGE_COLLECTION: &'static str = "external_messages";

/// Convenience trait. Converts a value to BSON.
trait ToBson {
    fn to_bson(&self) -> Result<Bson>;
    fn to_document(&self) -> Result<Document>;
}

impl<T: Serialize> ToBson for T {
    fn to_bson(&self) -> Result<Bson> {
        Ok(to_bson(self)?)
    }
    fn to_document(&self) -> Result<Document> {
        Ok(to_document(self)?)
    }
}

pub enum VerifyOutcome {
    FieldOk,
    FullyVerified,
}

pub struct Database {
    db: MongoDb,
    last_message_check: Timestamp,
}

impl Database {
    pub async fn new(uri: &str, db: &str) -> Result<Self> {
        Ok(Database {
            db: Client::with_uri_str(uri).await?.database(db),
            last_message_check: Timestamp::now(),
        })
    }
    pub fn add_judgement_request(&self, request: JudgementState) -> Result<()> {
        unimplemented!()
    }
    pub async fn add_message(&self, message: ExternalMessage) -> Result<()> {
        let coll = self.db.collection(MESSAGE_COLLECTION);

        coll.update_one(
            doc! {
                "type": message.ty.to_bson()?,
                "id": message.id.to_bson()?,
            },
            message.to_document()?,
            Some({
                let mut options = UpdateOptions::default();
                options.upsert = Some(true);
                options
            }),
        )
        .await?;

        Ok(())
    }
    pub async fn add_chain_remark(&self, remark: ChainRemark) -> Result<()> {
        unimplemented!()
    }
    pub async fn fetch_messages(&self) -> Result<Vec<ExternalMessage>> {
        let coll = self.db.collection(MESSAGE_COLLECTION);

        // Read the latest, unprocessed messages.
        let mut cursor = coll.find(
            doc! {
                "timestamp": {
                    "$gt": self.last_message_check.to_bson()?,
                }
            },
            None,
        )
        .await?;

        // Parse and collect messages.
        let mut messages = vec![];
        while let Some(doc) = cursor.next().await {
            messages.push(from_document(doc?)?);
        }

        Ok(messages)
    }
    pub async fn verify_message(&self, message: ExternalMessage) -> Result<VerifyOutcome> {
        unimplemented!()
    }
    pub async fn set_validity() -> Result<()> {
        unimplemented!()
    }
    pub async fn fetch_judgement_state(
        &self,
        chain_address: IdentityContext,
    ) -> Result<JudgementState> {
        unimplemented!()
    }
}
