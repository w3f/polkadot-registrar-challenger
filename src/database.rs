use crate::primitives::{
    ChainAddress, ChainRemark, Event, ExternalMessage, IdentityContext, IdentityField,
    JudgementState, MessageId, NotificationMessage, Timestamp,
};
use crate::Result;
use bson::{doc, from_document, to_bson, to_document, Bson, Document};
use futures::StreamExt;
use mongodb::{options::UpdateOptions, Client, Database as MongoDb};
use serde::Serialize;
use std::collections::HashSet;

const IDENTITY_COLLECTION: &'static str = "identities";
const EVENT_COLLECTION: &'static str = "event_log";

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
        let coll = self.db.collection(IDENTITY_COLLECTION);

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
    pub async fn verify_message(&self, message: ExternalMessage) -> Result<bool> {
        let coll = self.db.collection(IDENTITY_COLLECTION);

        // Fetch the current field state based on the message origin.
        let mut cursor = coll
            .find(
                doc! {
                    "fields.value": message.origin.to_bson()?,
                },
                None,
            )
            .await?;

        // If a field was found, update it.
        while let Some(doc) = cursor.next().await {
            let field_state: IdentityField = from_document(doc?)?;

            // Ignore if the field has already been verified.
            if field_state.is_verified {
                return Ok(false);
            }

            // If the message contains the challenge, set it as valid (or
            // invalid if otherwise).
            let event = if message.contains_challenge(&field_state.expected_challenge) {
                // Update field state. Be more specific with the query in order
                // to verify the correct field (in theory, there could be
                // multiple pending requests with the same external account
                // specified).
                coll.update_one(
                    doc! {
                        "fields.value": message.origin.to_bson()?,
                        "fields.expected_challenge": field_state.expected_challenge.to_bson()?,
                    },
                    doc! {
                        "fields.is_verified": true,
                    },
                    None,
                )
                .await?;

                NotificationMessage::FieldVerified(field_state.value.clone())
            } else {
                // Update field state.
                coll.update_one(
                    doc! {
                        "fields.value": message.origin.to_bson()?,
                    },
                    doc! {
                        "$inc": {
                            "fields.failed_attempts": 1
                        }
                    },
                    None,
                )
                .await?;

                NotificationMessage::FieldVerificationFailed(field_state.value.clone())
            };

            // Create event statement.
            let coll = self.db.collection(EVENT_COLLECTION);
            coll.insert_one(Event::from(event).to_document()?, None)
                .await?;

            if field_state.is_verified {
                return Ok(true);
            }
        }

        Ok(false)
    }
    pub async fn verify_remark(&self, remark: ChainRemark) -> Result<bool> {
        let coll = self.db.collection(IDENTITY_COLLECTION);

        let doc = coll
            .find_one(
                doc! {
                    "context": remark.context.to_bson()?,
                },
                None,
            )
            .await?;

        if let Some(doc) = doc {
            let mut id_state: JudgementState = from_document(doc)?;

            // Ignore if the identity has already been verified.
            if id_state.is_fully_verified {
                return Ok(false);
            }

            let event = if id_state.expected_remark.matches(&remark) {
                // Check if each field has already been verified. Technically,
                // this should never return (the expected on-chain remark is
                // sent *after* each field has been verified).
                if !id_state.check_field_verifications() {
                    return Ok(false);
                }

                id_state.is_fully_verified = true;

                Event::from(NotificationMessage::RemarkVerified(
                    id_state.context.clone(),
                    id_state.expected_remark.clone(),
                ))
            } else {
                id_state.failed_remark_attempts += 1;

                debug!("Failed remark verification for {:?}", remark.context);

                Event::from(NotificationMessage::RemarkVerificationFailed(
                    id_state.context.clone(),
                    id_state.expected_remark.clone(),
                ))
            };

            // Update state.
            // TODO: Only update individual fields.
            coll.update_one(
                doc! {
                    "context": remark.context.to_bson()?,
                },
                id_state.to_document()?,
                None,
            )
            .await?;

            // Create event statement.
            let coll = self.db.collection(EVENT_COLLECTION);
            coll.insert_one(event.to_document()?, None).await?;

            if id_state.is_fully_verified {
                return Ok(true);
            }
        } else {
        }

        Ok(false)
    }
    pub async fn fetch_judgement_state(
        &self,
        context: IdentityContext,
    ) -> Result<Option<JudgementState>> {
        let coll = self.db.collection(IDENTITY_COLLECTION);

        // Find the context.
        let doc = coll
            .find_one(
                doc! {
                    "context": context.to_bson()?,
                },
                None,
            )
            .await?;

        if let Some(doc) = doc {
            Ok(Some(from_document(doc)?))
        } else {
            // Not active request exists.
            Ok(None)
        }
    }
    pub async fn log_judgement_provided(&self, context: IdentityContext) -> Result<()> {
        let coll = self.db.collection(EVENT_COLLECTION);

        coll.insert_one(
            Event::from(NotificationMessage::JudgementProvided(context)).to_document()?,
            None,
        )
        .await?;

        Ok(())
    }
}
