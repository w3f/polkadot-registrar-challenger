use crate::actors::api::VerifyChallenge;
use crate::actors::connector::DisplayNameEntry;
use crate::primitives::{
    ChainName, ChallengeType, Event, ExpectedMessage, ExternalMessage, IdentityContext,
    IdentityFieldValue, JudgementState, NotificationMessage, Timestamp,
};
use crate::Result;
use bson::{doc, from_document, to_bson, to_document, Bson, Document};
use futures::StreamExt;
use mongodb::options::UpdateOptions;
use mongodb::{Client, Database as MongoDb};
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
        let event_log = self.db.collection::<Event>(EVENT_COLLECTION);

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
                    .find(|current| current.value() == new_field.value())
                {
                    to_add.push(current_field.clone());
                } else {
                    to_add.push(new_field.clone());
                }
            }

            // Check verification status.
            current.is_fully_verified = if !to_add.is_empty() {
                // (Re-)set validity if new fields are available.
                false
            } else {
                current.is_fully_verified
            };

            // Set new fields.
            current.fields = to_add;

            // Update the final value in database.
            // TODO: Check modified count.
            coll.update_one(
                doc! {
                    "context": request.context.to_bson()?
                },
                doc! {
                    "$set": {
                        "is_fully_verified": current.is_fully_verified.to_bson()?,
                        "fields": current.fields.to_bson()?
                    }
                },
                None,
            )
            .await?;

            // Create event.
            event_log
                .insert_one(
                    Event::new(NotificationMessage::IdentityUpdated {
                        context: request.context.clone(),
                    }),
                    None,
                )
                .await?;
        } else {
            coll.insert_one(request.to_document()?, None).await?;

            // Create event.
            event_log
                .insert_one(
                    Event::new(NotificationMessage::IdentityInserted {
                        context: request.context.clone(),
                    }),
                    None,
                )
                .await?;
        }

        Ok(())
    }
    // TODO: Merge with 'verify_message'?
    pub async fn process_message(&mut self, message: &ExternalMessage) -> Result<()> {
        let events = self.verify_message(message).await?;

        // Create event statement.
        let coll = self.db.collection(EVENT_COLLECTION);
        for event in events {
            coll.insert_one(Event::new(event).to_document()?, None)
                .await?;
        }

        Ok(())
    }
    async fn verify_message(&self, message: &ExternalMessage) -> Result<Vec<NotificationMessage>> {
        // TODO: Specify type on `collection`.
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

        let mut events = vec![];

        // If a field was found, update it.
        while let Some(doc) = cursor.next().await {
            let mut id_state: JudgementState = from_document(doc?)?;
            let field_state = id_state
                .fields
                .iter_mut()
                .find(|field| field.value().matches(&message))
                // Technically, this should never return an error...
                .ok_or(anyhow!("Failed to select field when verifying message"))?;

            // If the message contains the challenge, set it as valid (or
            // invalid if otherwise).

            let context = id_state.context.clone();
            let field_value = field_state.value().clone();

            let challenge = field_state.challenge_mut();
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
                                // TODO: Filter by context?
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

                                events.push(NotificationMessage::FieldVerified {
                                    context: context.clone(),
                                    field: field_value.clone(),
                                });

                                // TODO: Document
                                if second.is_some() {
                                    events.push(NotificationMessage::AwaitingSecondChallenge {
                                        context: context.clone(),
                                        field: field_value,
                                    });
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

                                events.push(NotificationMessage::FieldVerificationFailed {
                                    context: context.clone(),
                                    field: field_value,
                                });
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

            std::mem::drop(field_state);

            // Check if the identity is fully verified.
            self.process_fully_verified(&id_state)
                .await?
                .map(|event| events.push(event));
        }

        Ok(events)
    }
    /// Check if all fields have been verified.
    pub async fn process_fully_verified(
        &self,
        state: &JudgementState,
    ) -> Result<Option<NotificationMessage>> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        if state.is_fully_verified() {
            // Create a timed delay for issuing judgments. Between 30 seconds to 5 minutes.
            // TODO: Explain reasoning.
            let now = Timestamp::now();
            let offset = thread_rng().gen_range(30, 300);
            let issue_at = Timestamp::with_offset(offset);

            coll.update_one(
                doc! {
                    "context": state.context.to_bson()?,
                },
                doc! {
                    "$set": {
                        "is_fully_verified": true.to_bson()?,
                        "completion_timestamp": now.to_bson()?,
                        "issue_judgement_at": issue_at.to_bson()?,
                    }
                },
                None,
            )
            .await?;

            return Ok(Some(NotificationMessage::IdentityFullyVerified {
                context: state.context.clone(),
            }));
        }

        Ok(None)
    }
    pub async fn verify_second_challenge(&mut self, mut request: VerifyChallenge) -> Result<bool> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let mut verified = false;
        let mut events = vec![];

        // Trim received challenge, just in case.
        request.challenge = request.challenge.trim().to_string();

        // Query database.
        let try_state = coll
            .find_one(
                doc! {
                    "fields.value": request.entry.to_bson()?,
                    "fields.challenge.content.second.value": request.challenge.to_bson()?,
                },
                None,
            )
            .await?;

        if let Some(mut state) = try_state {
            let field_state = state
                .fields
                .iter_mut()
                .find(|field| field.value() == &request.entry)
                // Technically, this should never return an error...
                .ok_or(anyhow!("Failed to select field when verifying message"))?;

            let context = state.context.clone();
            let field_value = field_state.value().clone();

            match field_state.challenge_mut() {
                ChallengeType::ExpectedMessage {
                    expected: _,
                    second,
                } => {
                    let second = second.as_mut().unwrap();
                    if request.challenge.contains(&second.value) {
                        second.set_verified();
                        verified = true;

                        // TODO: Filter by context?
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

                        events.push(NotificationMessage::SecondFieldVerified {
                            context: context.clone(),
                            field: field_value.clone(),
                        });
                    } else {
                        events.push(NotificationMessage::SecondFieldVerificationFailed {
                            context: context.clone(),
                            field: field_value.clone(),
                        });
                    }
                }
                _ => {
                    // TODO: Should panic
                    return Err(anyhow!("Invalid challenge type when verifying message"));
                }
            }

            // Check if the identity is fully verified.
            self.process_fully_verified(&state)
                .await?
                .map(|event| events.push(event));
        }

        let coll = self.db.collection(EVENT_COLLECTION);
        for event in events {
            coll.insert_one(Event::new(event).to_document()?, None)
                .await?;
        }

        Ok(verified)
    }
    pub async fn fetch_second_challenge(
        &self,
        field: &IdentityFieldValue,
    ) -> Result<ExpectedMessage> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        // Query database.
        let try_state = coll
            .find_one(
                doc! {
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
                .find(|f| f.value() == field)
                // Technically, this should never return an error...
                .ok_or(anyhow!("Failed to select field when verifying message"))?;

            match field_state.challenge() {
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
                        "$gt": after,
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
        let event_log = self.db.collection::<Event>(EVENT_COLLECTION);

        coll.update_one(
            doc! {
                "context": context.to_bson()?,
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
        event_log
            .insert_one(
                Event::new(NotificationMessage::JudgementProvided {
                    context: context.clone(),
                }),
                None,
            )
            .await?;

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
    pub async fn set_display_name_valid(&self, context: &IdentityContext) -> Result<()> {
        let coll = self.db.collection::<()>(IDENTITY_COLLECTION);

        coll.update_one(
            doc! {
                "context": context.to_bson()?,
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

        Ok(())
    }
    // TODO: Consider creating an event.
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
}
