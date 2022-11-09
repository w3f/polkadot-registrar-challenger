use crate::adapters::admin::RawFieldName;
use crate::api::VerifyChallenge;
use crate::connector::DisplayNameEntry;
use crate::primitives::{
    ChainName, ChallengeType, Event, ExpectedMessage, ExternalMessage, IdentityContext,
    IdentityFieldValue, JudgementState, NotificationMessage, Timestamp,
};
use crate::Result;
use bson::{doc, from_document, to_bson, to_document, Bson, Document};
use futures::StreamExt;
use mongodb::options::{TransactionOptions, UpdateOptions};
use mongodb::{Client, ClientSession, Database as MongoDb};
use rand::{thread_rng, Rng};
use serde::Serialize;
use std::collections::HashMap;
use std::time::Duration;

const IDENTITY_COLLECTION: &str = "identities";
const EVENT_COLLECTION: &str = "event_log";
const DISPLAY_NAMES: &str = "display_names";

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

// Keeps track of the latest, fetched events to avoid sending old messages or
// duplicates.
pub struct EventCursor {
    timestamp: Timestamp,
    fetched_ids: HashMap<String, Timestamp>,
}

impl EventCursor {
    pub fn new() -> Self {
        EventCursor {
            timestamp: Timestamp::now(),
            fetched_ids: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Database {
    client: Client,
    db: MongoDb,
}

impl Database {
    pub async fn new(uri: &str, db: &str) -> Result<Self> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(db);

        Ok(Database { client, db })
    }
    async fn start_transaction(&self) -> Result<ClientSession> {
        let mut options = TransactionOptions::default();
        options.max_commit_time = Some(Duration::from_secs(30));

        let mut session = self.client.start_session(None).await?;
        session.start_transaction(Some(options)).await?;
        Ok(session)
    }
    /// Simply checks if a connection could be established to the database.
    pub async fn connectivity_check(&self) -> Result<()> {
        self.db
            .list_collection_names(None)
            .await
            .map_err(|err| anyhow!("Failed to connect to database: {:?}", err))
            .map(|_| ())
    }
    pub async fn add_judgement_request(&self, request: &JudgementState) -> Result<bool> {
        let mut session = self.start_transaction().await?;
        let coll = self.db.collection(IDENTITY_COLLECTION);

        // Check if a request of the same address exists yet (occurs when a
        // field gets updated during pending judgement process).
        let doc = coll
            .find_one_with_session(
                doc! {
                    "context": request.context.to_bson()?,
                },
                None,
                &mut session,
            )
            .await?;

        // If it does exist, only update specific fields.
        if let Some(doc) = doc {
            let mut current: JudgementState = from_document(doc)?;

            // Determine which fields should be updated.
            let mut has_changed = false;
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
                    has_changed = true;
                }
            }

            // If nothing was modified, return (detect removed entries).
            if !has_changed && request.fields.len() == current.fields.len() {
                return Ok(false);
            }

            // Set new fields.
            current.fields = to_add;

            // Update the final fields in the database. All deprecated fields
            // are overwritten.
            coll.update_one_with_session(
                doc! {
                    "context": request.context.to_bson()?
                },
                doc! {
                    "$set": {
                        "fields": current.fields.to_bson()?
                    }
                },
                None,
                &mut session,
            )
            .await?;

            // Create event.
            self.insert_event(
                NotificationMessage::IdentityUpdated {
                    context: request.context.clone(),
                },
                &mut session,
            )
            .await?;

            // Check full verification status.
            self.process_fully_verified(&current, &mut session).await?;
        } else {
            // Insert new identity.
            coll.update_one_with_session(
                doc! {
                    "context": request.context.to_bson()?,
                },
                doc! {
                    "$setOnInsert": request.to_document()?,
                },
                {
                    let mut opt = UpdateOptions::default();
                    opt.upsert = Some(true);
                    Some(opt)
                },
                &mut session,
            )
            .await?;
        }

        session.commit_transaction().await?;

        Ok(true)
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
        // Whether it should check if the idenity has been fully verified.
        full_check: bool,
        provided_session: Option<&mut ClientSession>,
    ) -> Result<Option<()>> {
        // If no `session` is provided, create a new local session.
        let mut local_session = self.start_transaction().await?;
        let should_commit;

        let session = if let Some(session) = provided_session {
            should_commit = false;
            local_session.abort_transaction().await?;
            std::mem::drop(local_session);
            session
        } else {
            should_commit = true;
            &mut local_session
        };

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
            RawFieldName::All => {
                return Err(anyhow!(
                    "field name 'all' is abstract and cannot be verified individually"
                ))
            }
        };

        // Update field.
        let res = coll
            .update_one_with_session(
                doc! {
                    "context": context.to_bson()?,
                    "fields.value.type": field.to_string(),
                },
                update,
                None,
                session,
            )
            .await?;

        if res.modified_count == 0 {
            return Ok(None);
        }

        // Create event.
        if full_check {
            self.insert_event(
                NotificationMessage::ManuallyVerified {
                    context: context.clone(),
                    field: field.clone(),
                },
                session,
            )
            .await?;

            // Get the full state.
            let doc = coll
                .find_one_with_session(
                    doc! {
                        "context": context.to_bson()?,
                    },
                    None,
                    session,
                )
                .await?;

            // Check the new state.
            if let Some(state) = doc {
                self.process_fully_verified(&state, session).await?;
            } else {
                return Ok(None);
            }
        }

        if should_commit {
            session.commit_transaction().await?;
        }

        Ok(Some(()))
    }
    pub async fn verify_message(&self, message: &ExternalMessage) -> Result<()> {
        let mut session = self.start_transaction().await?;
        let coll = self.db.collection(IDENTITY_COLLECTION);

        // Fetch the current field state based on the message origin.
        let mut cursor = coll
            .find_with_session(
                doc! {
                    "fields.value": message.origin.to_bson()?,
                },
                None,
                &mut session,
            )
            .await?;

        // If a field was found, update it.
        while let Some(doc) = cursor.next(&mut session).await {
            let mut id_state: JudgementState = from_document(doc?)?;
            let field_state = id_state
                .fields
                .iter_mut()
                .find(|field| field.value.matches_origin(message))
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
                            if expected.verify_message(message) {
                                // Update field state. Be more specific with the query in order
                                // to verify the correct field (in theory, there could be
                                // multiple pending requests with the same external account
                                // specified).
                                coll.update_one_with_session(
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
                                    &mut session,
                                )
                                .await?;

                                self.insert_event(
                                    NotificationMessage::FieldVerified {
                                        context: context.clone(),
                                        field: field_value.clone(),
                                    },
                                    &mut session,
                                )
                                .await?;

                                if second.is_some() {
                                    self.insert_event(
                                        NotificationMessage::AwaitingSecondChallenge {
                                            context: context.clone(),
                                            field: field_value,
                                        },
                                        &mut session,
                                    )
                                    .await?;
                                }
                            } else {
                                // Update field state.
                                coll.update_many_with_session(
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
                                    &mut session,
                                )
                                .await?;

                                self.insert_event(
                                    NotificationMessage::FieldVerificationFailed {
                                        context: context.clone(),
                                        field: field_value,
                                    },
                                    &mut session,
                                )
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
            self.process_fully_verified(&id_state, &mut session).await?;
        }

        session.commit_transaction().await?;

        Ok(())
    }
    /// Check if all fields have been verified.
    async fn process_fully_verified(
        &self,
        state: &JudgementState,
        session: &mut ClientSession,
    ) -> Result<()> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        if state.check_full_verification() {
            // Create a timed delay for issuing judgments. Between 30 seconds to
            // 5 minutes. This is used to prevent timing attacks where a user
            // updates the identity right before the judgement is issued.
            let now = Timestamp::now();
            let offset = thread_rng().gen_range(30..300);
            let issue_at = Timestamp::with_offset(offset);

            let res = coll
                .update_one_with_session(
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
                    session,
                )
                .await?;

            if res.modified_count > 0 {
                self.insert_event(
                    NotificationMessage::IdentityFullyVerified {
                        context: state.context.clone(),
                    },
                    session,
                )
                .await?;
            }
        } else {
            // Reset verification state if identity was changed.
            let _ = coll
                .update_one_with_session(
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
                    session,
                )
                .await?;
        }

        Ok(())
    }
    pub async fn verify_second_challenge(&self, mut request: VerifyChallenge) -> Result<bool> {
        let mut session = self.start_transaction().await?;
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let mut verified = false;

        // Trim received challenge, just in case.
        request.challenge = request.challenge.trim().to_string();

        // Query database.
        let mut cursor = coll
            .find_with_session(
                doc! {
                    "fields.value": request.entry.to_bson()?,
                },
                None,
                &mut session,
            )
            .await?;

        while let Some(state) = cursor.next(&mut session).await {
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

                        coll.update_one_with_session(
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
                            &mut session
                        )
                        .await?;

                        self.insert_event(
                            NotificationMessage::SecondFieldVerified {
                                context: context.clone(),
                                field: field_value.clone(),
                            },
                            &mut session,
                        )
                        .await?;
                    } else {
                        self.insert_event(
                            NotificationMessage::SecondFieldVerificationFailed {
                                context: context.clone(),
                                field: field_value.clone(),
                            },
                            &mut session,
                        )
                        .await?;
                    }
                }
                _ => {
                    panic!("Invalid challenge type when verifying message");
                }
            }

            // Check if the identity is fully verified.
            self.process_fully_verified(&state, &mut session).await?;
        }

        session.commit_transaction().await?;

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
                .ok_or_else(|| anyhow!("Failed to select field when verifying message"))?;

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
        event_tracker: &mut EventCursor,
    ) -> Result<Vec<NotificationMessage>> {
        #[derive(Debug, Deserialize)]
        struct EventWrapper {
            #[serde(rename = "_id")]
            id: bson::oid::ObjectId,
            #[serde(flatten)]
            event: Event,
        }

        let coll = self.db.collection(EVENT_COLLECTION);

        let mut cursor = coll
            .find(
                doc! {
                    "timestamp": {
                        "$gte": event_tracker.timestamp.raw().to_bson()?,
                    }
                },
                None,
            )
            .await?;

        let mut events = vec![];

        while let Some(doc) = cursor.next().await {
            let wrapper = from_document::<EventWrapper>(doc?)?;
            let hex_id = wrapper.id.to_hex();

            if event_tracker.fetched_ids.contains_key(&hex_id) {
                continue;
            }

            // Save event
            let timestamp = wrapper.event.timestamp;
            events.push(wrapper);

            // Track event in EventCursor
            event_tracker.fetched_ids.insert(hex_id, timestamp);
            event_tracker.timestamp = event_tracker.timestamp.max(timestamp);
        }

        // Clean cache, only keep ids of the last 10 seconds.
        let current = event_tracker.timestamp.raw();
        event_tracker
            .fetched_ids
            .retain(|_, timestamp| timestamp.raw() > current - 10);

        // Sort by id, ascending.
        events.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(events
            .into_iter()
            .map(|wrapper| wrapper.event.message)
            .collect())
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
    pub async fn fetch_judgement_candidates(
        &self,
        network: ChainName,
    ) -> Result<Vec<JudgementState>> {
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let mut cursor = coll
            .find(
                doc! {
                    "context.chain": network.as_str().to_bson()?,
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
    // (Warning) This fully verifies the identity without having to verify
    // individual fields.
    pub async fn full_manual_verification(&self, context: &IdentityContext) -> Result<bool> {
        let mut session = self.start_transaction().await?;
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        // Create a timed delay for issuing judgments. Between 30 seconds to
        // 5 minutes. This is used to prevent timing attacks where a user
        // updates the identity right before the judgement is issued.
        let now = Timestamp::now();
        let offset = thread_rng().gen_range(30..300);
        let issue_at = Timestamp::with_offset(offset);

        let res = coll
            .update_one_with_session(
                doc! {
                    "context": context.to_bson()?,
                },
                doc! {
                    "$set": {
                        "is_fully_verified": true,
                        "judgement_submitted": false,
                        "completion_timestamp": now.to_bson()?,
                        "issue_judgement_at": issue_at.to_bson()?,
                    }
                },
                None,
                &mut session,
            )
            .await?;

        // Create event.
        if res.modified_count == 1 {
            // Verify all possible fields. Unused fields are silently ignored.
            let _ = self
                .verify_manually(context, &RawFieldName::LegalName, false, Some(&mut session))
                .await?;
            let _ = self
                .verify_manually(
                    context,
                    &RawFieldName::DisplayName,
                    false,
                    Some(&mut session),
                )
                .await?;
            let _ = self
                .verify_manually(context, &RawFieldName::Email, false, Some(&mut session))
                .await?;
            let _ = self
                .verify_manually(context, &RawFieldName::Web, false, Some(&mut session))
                .await?;
            let _ = self
                .verify_manually(context, &RawFieldName::Twitter, false, Some(&mut session))
                .await?;
            let _ = self
                .verify_manually(context, &RawFieldName::Matrix, false, Some(&mut session))
                .await?;

            self.insert_event(
                NotificationMessage::FullManualVerification {
                    context: context.clone(),
                },
                &mut session,
            )
            .await?;

            session.commit_transaction().await?;
            Ok(true)
        } else {
            session.commit_transaction().await?;
            Ok(false)
        }
    }
    pub async fn set_judged(&self, context: &IdentityContext) -> Result<()> {
        let mut session = self.start_transaction().await?;
        let coll = self.db.collection::<JudgementState>(IDENTITY_COLLECTION);

        let res = coll
            .update_one_with_session(
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
                &mut session,
            )
            .await?;

        // Create event.
        if res.modified_count == 1 {
            self.insert_event(
                NotificationMessage::JudgementProvided {
                    context: context.clone(),
                },
                &mut session,
            )
            .await?;
        }

        session.commit_transaction().await?;

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
        let mut session = self.start_transaction().await?;
        let coll = self.db.collection::<()>(IDENTITY_COLLECTION);

        coll.update_one_with_session(
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
            &mut session,
        )
        .await?;

        // Create event
        self.insert_event(
            NotificationMessage::FieldVerified {
                context: state.context.clone(),
                field: state
                    .fields
                    .iter()
                    .find(|field| matches!(field.value, IdentityFieldValue::DisplayName(_)))
                    .map(|field| field.value.clone())
                    .expect("Failed to retrieve display name. This is a bug"),
            },
            &mut session,
        )
        .await?;

        self.process_fully_verified(state, &mut session).await?;

        session.commit_transaction().await?;

        Ok(())
    }
    pub async fn insert_display_name_violations(
        &self,
        context: &IdentityContext,
        violations: &Vec<DisplayNameEntry>,
    ) -> Result<()> {
        let mut session = self.start_transaction().await?;
        let coll = self.db.collection::<()>(IDENTITY_COLLECTION);

        coll.update_one_with_session(
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
            &mut session,
        )
        .await?;

        session.commit_transaction().await?;

        Ok(())
    }
    async fn insert_event<T: Into<Event>>(
        &self,
        event: T,
        session: &mut ClientSession,
    ) -> Result<()> {
        let coll = self.db.collection(EVENT_COLLECTION);

        let event = <T as Into<Event>>::into(event);
        coll.insert_one_with_session(event.to_bson()?, None, session)
            .await?;

        Ok(())
    }
}
