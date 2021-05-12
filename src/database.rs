use crate::primitives::{
    ChainAddress, Event, ExternalMessage, IdentityContext, IdentityField, JudgementState,
    MessageId, NotificationMessage, Timestamp,
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

pub enum VerificationOutcome {
    AlreadyVerified,
    Valid {
        state: JudgementState,
        notifications: Vec<NotificationMessage>,
    },
    Invalid {
        state: JudgementState,
        notifications: Vec<NotificationMessage>,
    },
    NotFound,
}

#[derive(Debug, Clone)]
pub struct Database {
    db: MongoDb,
}

impl Database {
    pub async fn new(uri: &str, db: &str) -> Result<Self> {
        Ok(Database {
            db: Client::with_uri_str(uri).await?.database(db),
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
    pub async fn verify_message(&self, message: &ExternalMessage) -> Result<VerificationOutcome> {
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
            let id_state: JudgementState = from_document(doc?)?;
            let field_state = id_state
                .fields
                .iter()
                .find(|field| field.value.matches(&message))
                // Technically, this should never return an error...
                .ok_or(anyhow!("Failed to select field when verifying message"))?;

            // Ignore if the field has already been verified.
            if field_state.is_verified {
                return Ok(VerificationOutcome::AlreadyVerified);
            }

            // If the message contains the challenge, set it as valid (or
            // invalid if otherwise).
            let notification = if message.contains_challenge(&field_state.expected_challenge) {
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

                NotificationMessage::FieldVerified(
                    id_state.context.clone(),
                    field_state.value.clone(),
                )
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
            coll.insert_one(Event::from(notification.clone()).to_document()?, None)
                .await?;

            // If it was verified, there no longer is a need to continue
            // verification.
            if field_state.is_verified {
                return Ok(VerificationOutcome::Valid {
                    state: id_state.clone(),
                    notifications: vec![notification],
                });
            } else {
                return Ok(VerificationOutcome::Invalid {
                    state: id_state.clone(),
                    notifications: vec![notification],
                });
            }
        }

        Ok(VerificationOutcome::NotFound)
    }
    pub async fn fetch_judgement_state(
        &self,
        context: &IdentityContext,
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
