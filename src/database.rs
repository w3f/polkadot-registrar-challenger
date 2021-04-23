use crate::primitives::{
    ChainAddress, ChainRemark, ExternalMessage, IdentityContext, JudgementState, MessageId,
    Timestamp,
};
use crate::Result;
use bson::{doc, from_document, to_bson, to_document, Bson, Document};
use futures::StreamExt;
use mongodb::{options::UpdateOptions, Client, Database as MongoDb};
use serde::Serialize;
use std::collections::HashSet;

const IDENTITY_COLLECTION: &'static str = "identities";
const MESSAGE_COLLECTION: &'static str = "external_messages";
const REMARK_COLLECTION: &'static str = "on_chain_remark";

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
    pub async fn add_judgement_request(&self, request: JudgementState) -> Result<()> {
        let coll = self.db.collection(MESSAGE_COLLECTION);

        // Check if a request of the same address exists yet (occurs when a
        // field gets updated during pending judgement process).
        let doc = coll
            .find_one(
                doc! {
                    "context": request.context.to_bson()?,
                },
                None,
            )
            .await?;

        // If it does exist, only update specific fields.
        if let Some(doc) = doc {
            let mut current: JudgementState = from_document(doc)?;

            // Determine which fields should be updated.
            let mut to_add = vec![];
            for new_field in request.fields {
                // If the current field value is the same as the new one, insert
                // the current field state back into storage. If the value is
                // new, insert/update the current field state.
                if let Some(current_field) = current
                    .fields
                    .iter()
                    .find(|current| current.value == new_field.value)
                {
                    to_add.push(current_field.clone());
                } else {
                    to_add.push(new_field);
                }
            }

            // Set new fields.
            current.fields = to_add;

            // Update the final value in database.
            coll.update_one(
                doc! {
                    "context": request.context.to_bson()?
                },
                current.to_document()?,
                None,
            )
            .await?;
        } else {
            coll.insert_one(request.to_document()?, None).await?;
        }

        Ok(())
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
        let coll = self.db.collection(REMARK_COLLECTION);

        coll.insert_one(remark.to_document()?, None).await?;

        Ok(())
    }
    pub async fn fetch_messages(&self) -> Result<Vec<ExternalMessage>> {
        let coll = self.db.collection(MESSAGE_COLLECTION);

        // Read the latest, unprocessed messages.
        let mut cursor = coll
            .find(
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
    pub async fn verify_remark(&self, remark: ChainRemark) -> Result<()> {
        unimplemented!()
    }
    pub async fn fetch_judgement_state(
        &self,
        chain_address: IdentityContext,
    ) -> Result<JudgementState> {
        unimplemented!()
    }
}
