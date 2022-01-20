use crate::actors::api::VerifyChallenge;
use crate::actors::connector::DisplayNameEntry;
use crate::adapters::admin::RawFieldName;
use crate::primitives::{
    ChainName, ChallengeType, Event, ExpectedMessage, ExternalMessage, IdentityContext,
    IdentityFieldValue, JudgementState, JudgementStateBlanked, NotificationMessage, Timestamp,
};
use crate::Result;
use bson::{doc, from_document, to_bson, to_document, Bson, Document};
use futures::StreamExt;
use mongodb::options::UpdateOptions;
use mongodb::{Client, Database as MongoDb};
use rand::distributions::uniform::UniformSampler;
use rand::{thread_rng, Rng};
use serde::Serialize;

const IDENTITY_COLLECTION: &'static str = "identities";
const EVENT_COLLECTION: &'static str = "event_log";
const DISPLAY_NAMES: &'static str = "display_names";

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
    pub async fn add_judgement_request(&self, request: &JudgementState) -> Result<()> {
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
            for new_field in &request.fields {
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
                    to_add.push(new_field.clone());
                }
            }

            // Set new fields.
            current.fields = to_add;

            // Update the final value in database.
            coll.update_one(
                doc! {
                    "context": request.context.to_bson()?
                },
                doc! {
                    "$set": {
                        "fields": current.fields.to_bson()?
                    }
                },
                None,
            )
            .await?;

            // Create event.
            self.insert_event(NotificationMessage::IdentityUpdated {
                context: request.context.clone(),
            })
            .await?;

            // Check full verification status.
            self.process_fully_verified(&current).await?;
        } else {
            coll.insert_one(request.to_document()?, None).await?;
        }

        Ok(())
    }
    #[cfg(test)]
    pub async fn delete_judgement(&self, context: &IdentityContext) -> Result<()> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let res = coll
            .delete_one(
                doc! {
                    "context": context.to_bson()?,
                },
                None,
            )
            .await?;

        if res.deleted_count != 1 {
            panic!()
        }

        Ok(())
    }
    pub async fn verify_manually(
        &self,
        context: &IdentityContext,
        field: &RawFieldName,
    ) -> Result<Option<()>> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        // Set the appropriate types for verification.
        let update = match field {
            // For "ChallengeType::ExpectedMessage".
            RawFieldName::Twitter | RawFieldName::Matrix => {
                doc! {
                    "$set": {
                        "fields.$.challenge.content.expected.is_verified": true,
                    }
                }
            }
            // For "ChallengeType::ExpectedMessage" (with secondary verification).
            RawFieldName::Email => {
                doc! {
                    "$set": {
                        "fields.$.challenge.content.expected.is_verified": true,
                        "fields.$.challenge.content.second.is_verified": true,
                    }
                }
            }
            // For "ChallengeType::DisplayNameCheck".
            RawFieldName::DisplayName => {
                doc! {
                    "$set": {
                        "fields.$.challenge.content.passed": true,
                    }
                }
            }
            // For "ChallengeType::Unsupported".
            RawFieldName::LegalName | RawFieldName::Web => {
                doc! {
                    "$set": {
                        "fields.$.challenge.content.is_verified": true,
                    }
                }
            }
        };

        // Update field.
        let res = coll
            .update_one(
                doc! {
                    "context": context.to_bson()?,
                    "fields.value.type": field.to_string(),
                },
                update,
                None,
            )
            .await?;

        if res.modified_count != 1 {
            return Ok(None);
        }

        // Create event.
        self.insert_event(NotificationMessage::ManuallyVerified {
            context: context.clone(),
            field: field.clone(),
        })
        .await?;

        // Get the full state.
        let doc = coll
            .find_one(
                doc! {
                    "context": context.to_bson()?,
                },
                None,
            )
            .await?;

        // Check the new state.
        if let Some(state) = doc {
            self.process_fully_verified(&state).await?;
        } else {
            return Ok(None);
        }

        Ok(Some(()))
    }
    pub async fn verify_message(&self, message: &ExternalMessage) -> Result<()> {
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
            let mut id_state: JudgementState = from_document(doc?)?;
            let field_state = id_state
                .fields
                .iter_mut()
                .find(|field| field.value.matches(&message))
                .unwrap();

            // If the message contains the challenge, set it as valid (or
            // invalid if otherwise).

            let context = id_state.context.clone();
            let field_value = field_state.value.clone();

            let challenge = &mut field_state.challenge;
            if !challenge.is_verified() {
                match challenge {
                    ChallengeType::ExpectedMessage {
                        ref mut expected,
                        second,
                    } => {
                        // Only proceed if the expected challenge has not been verified yet.
                        if !expected.is_verified {
                            if expected.verify_message(&message) {
                                // Update field state. Be more specific with the query in order
                                // to verify the correct field (in theory, there could be
                                // multiple pending requests with the same external account
                                // specified).
                                coll.update_one(
                                    doc! {
                                        "context": context.to_bson()?,
                                        "fields.value": message.origin.to_bson()?,
                                    },
                                    doc! {
                                        "$set": {
                                            "fields.$.challenge.content.expected.is_verified": true,
                                        }
                                    },
                                    None,
                                )
                                .await?;

                                self.insert_event(NotificationMessage::FieldVerified {
                                    context: context.clone(),
                                    field: field_value.clone(),
                                })
                                .await?;

                                if second.is_some() {
                                    self.insert_event(
                                        NotificationMessage::AwaitingSecondChallenge {
                                            context: context.clone(),
                                            field: field_value,
                                        },
                                    )
                                    .await?;
                                }
                            } else {
                                // Update field state.
                                coll.update_many(
                                    doc! {
                                        "context": context.to_bson()?,
                                        "fields.value": message.origin.to_bson()?,
                                    },
                                    doc! {
                                        "$inc": {
                                            "fields.$.failed_attempts": 1isize.to_bson()?,
                                        }
                                    },
                                    None,
                                )
                                .await?;

                                self.insert_event(NotificationMessage::FieldVerificationFailed {
                                    context: context.clone(),
                                    field: field_value,
                                })
                                .await?;
                            }
                        }
                    }
                    _ => {
                        return Err(anyhow!(
                            "Invalid challenge type when verifying message. This is a bug"
                        ))
                    }
                }
            }

            // Check if the identity is fully verified.
            self.process_fully_verified(&id_state).await?;
        }

        Ok(())
    }
    /// Check if all fields have been verified.
    async fn process_fully_verified(&self, state: &JudgementState) -> Result<()> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        if state.check_full_verification() {
            // Create a timed delay for issuing judgments. Between 30 seconds to
            // 5 minutes. This is used to prevent timing attacks where a user
            // updates the identity right before the judgement is issued.
            let now = Timestamp::now();
            let offset = thread_rng().gen_range(30..300);
            let issue_at = Timestamp::with_offset(offset);

            let res = coll
                .update_one(
                    doc! {
                        "context": state.context.to_bson()?,
                        "is_fully_verified": false,
                    },
                    doc! {
                        "$set": {
                            "is_fully_verified": true,
                            "completion_timestamp": now.to_bson()?,
                            "issue_judgement_at": issue_at.to_bson()?,
                        }
                    },
                    None,
                )
                .await?;

            if res.modified_count > 0 {
                self.insert_event(NotificationMessage::IdentityFullyVerified {
                    context: state.context.clone(),
                })
                .await?;
            }
        } else {
            // Reset verification state if identity was changed.
            let _ = coll
                .update_one(
                    doc! {
                        "context": state.context.to_bson()?,
                        "is_fully_verified": true,
                    },
                    doc! {
                        "$set": {
                            "is_fully_verified": false,
                            "judgement_submitted": false,
                        }
                    },
                    None,
                )
                .await?;
        }

        Ok(())
    }
    pub async fn verify_second_challenge(&mut self, mut request: VerifyChallenge) -> Result<bool> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let mut verified = false;

        // Trim received challenge, just in case.
        request.challenge = request.challenge.trim().to_string();

        // Query database.
        let mut cursor = coll
            .find(
                doc! {
                    "fields.value": request.entry.to_bson()?,
                },
                None,
            )
            .await?;

        while let Some(state) = cursor.next().await {
            let mut state = state?;
            let field_state = state
                .fields
                .iter_mut()
                .find(|field| field.value == request.entry)
                .unwrap();

            let context = state.context.clone();
            let field_value = field_state.value.clone();

            match &mut field_state.challenge {
                ChallengeType::ExpectedMessage {
                    expected: _,
                    second,
                } => {
                    // This should never happens, but the provided field value
                    // depends on user input, so...
                    if second.is_none() {
                        continue;
                    }

                    let second = second.as_mut().unwrap();
                    if request.challenge.contains(&second.value) {
                        second.set_verified();
                        verified = true;

                        coll.update_one(
                            doc! {
                                "fields.value": request.entry.to_bson()?,
                                "fields.challenge.content.second.value": request.challenge.to_bson()?,
                            },
                            doc! {
                                "$set": {
                                    "fields.$.challenge.content.second.is_verified": true.to_bson()?,
                                }
                            },
                            None,
                        )
                        .await?;

                        self.insert_event(NotificationMessage::SecondFieldVerified {
                            context: context.clone(),
                            field: field_value.clone(),
                        })
                        .await?;
                    } else {
                        self.insert_event(NotificationMessage::SecondFieldVerificationFailed {
                            context: context.clone(),
                            field: field_value.clone(),
                        })
                        .await?;
                    }
                }
                _ => {
                    panic!("Invalid challenge type when verifying message");
                }
            }

            // Check if the identity is fully verified.
            self.process_fully_verified(&state).await?;
        }

        Ok(verified)
    }
    pub async fn fetch_second_challenge(
        &self,
        context: &IdentityContext,
        field: &IdentityFieldValue,
    ) -> Result<ExpectedMessage> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        // Query database.
        let try_state = coll
            .find_one(
                doc! {
                    "context": context.to_bson()?,
                    "fields.value": field.to_bson()?,
                },
                None,
            )
            .await?;

        if let Some(state) = try_state {
            // Optimize this. Should be handled by the query itself.
            let field_state = state
                .fields
                .iter()
                .find(|f| &f.value == field)
                // Technically, this should never return an error...
                .ok_or(anyhow!("Failed to select field when verifying message"))?;

            match &field_state.challenge {
                ChallengeType::ExpectedMessage {
                    expected: _,
                    second,
                } => {
                    if let Some(second) = second {
                        Ok(second.clone())
                    } else {
                        Err(anyhow!("No second challenge found for {:?}", field))
                    }
                }
                _ => Err(anyhow!("No second challenge found for {:?}", field)),
            }
        } else {
            Err(anyhow!("No entry found for {:?}", field))
        }
    }
    pub async fn fetch_events(
        &mut self,
        mut after: u64,
    ) -> Result<(Vec<NotificationMessage>, u64)> {
        let coll = self.db.collection(EVENT_COLLECTION);

        let mut cursor = coll
            .find(
                doc! {
                    "timestamp": {
                        "$gte": after.to_bson()?,
                    }
                },
                None,
            )
            .await?;

        let mut events = vec![];
        while let Some(doc) = cursor.next().await {
            let event = from_document::<Event>(doc?)?;

            // Track latest Id.
            after = after.max(event.timestamp.raw());
            events.push(event.event);
        }

        Ok((events, after))
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
    pub async fn fetch_judgement_candidates(&self) -> Result<Vec<JudgementState>> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let mut cursor = coll
            .find(
                doc! {
                    "is_fully_verified": true,
                    "judgement_submitted": false,
                    "issue_judgement_at": {
                        "$lt": Timestamp::now().to_bson()?,
                    }
                },
                None,
            )
            .await?;

        let mut completed = vec![];
        while let Some(state) = cursor.next().await {
            completed.push(state?);
        }

        Ok(completed)
    }
    pub async fn set_judged(&self, context: &IdentityContext) -> Result<()> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let res = coll
            .update_one(
                doc! {
                    "context": context.to_bson()?,
                    "judgement_submitted": false,
                },
                doc! {
                    "$set": {
                        "judgement_submitted": true,
                    }
                },
                None,
            )
            .await?;

        // Create event.
        if res.modified_count > 0 {
            self.insert_event(NotificationMessage::JudgementProvided {
                context: context.clone(),
            })
            .await?;
        }

        Ok(())
    }
    pub async fn insert_display_name(&self, name: &DisplayNameEntry) -> Result<()> {
        let coll = self.db.collection::<DisplayNameEntry>(DISPLAY_NAMES);

        coll.update_one(
            doc! {
                "display_name": name.display_name.to_bson()?,
                "context": name.context.to_bson()?,
            },
            doc! {
                "$setOnInsert": name.to_bson()?,
            },
            {
                let mut opt = UpdateOptions::default();
                opt.upsert = Some(true);
                Some(opt)
            },
        )
        .await?;

        Ok(())
    }
    pub async fn fetch_display_names(&self, chain: ChainName) -> Result<Vec<DisplayNameEntry>> {
        let coll = self.db.collection::<DisplayNameEntry>(DISPLAY_NAMES);

        let mut cursor = coll
            .find(
                doc! {
                    "context.chain": chain.to_bson()?,
                },
                None,
            )
            .await?;

        let mut names = vec![];
        while let Some(doc) = cursor.next().await {
            names.push(doc?);
        }

        Ok(names)
    }
    pub async fn set_display_name_valid(&self, state: &JudgementState) -> Result<()> {
        let coll = self.db.collection::<()>(IDENTITY_COLLECTION);

        coll.update_one(
            doc! {
                "context": state.context.to_bson()?,
                "fields.value.type": "display_name",
            },
            doc! {
                "$set": {
                    "fields.$.challenge.content.passed": true,
                }
            },
            None,
        )
        .await?;

        self.process_fully_verified(&state).await?;

        Ok(())
    }
    pub async fn insert_display_name_violations(
        &self,
        context: &IdentityContext,
        violations: &Vec<DisplayNameEntry>,
    ) -> Result<()> {
        let coll = self.db.collection::<()>(IDENTITY_COLLECTION);

        coll.update_one(
            doc! {
                "context": context.to_bson()?,
                "fields.value.type": "display_name",
            },
            doc! {
                "$set": {
                    "fields.$.challenge.content.passed": false,
                    "fields.$.challenge.content.violations": violations.to_bson()?
                }
            },
            None,
        )
        .await?;

        Ok(())
    }
    async fn insert_event<T: Into<Event>>(&self, event: T) -> Result<()> {
        let coll = self.db.collection(EVENT_COLLECTION);

        let event: Event = event.into();
        coll.insert_one(event.to_bson()?, None).await?;

        Ok(())
    }
}
