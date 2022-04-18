use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayNameEntry {
    pub context: IdentityContext,
    pub display_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum RawFieldName {
    LegalName,
    DisplayName,
    Email,
    Web,
    Twitter,
    Matrix,
}
// ***

impl From<IdentityState> for IdentityStateBlanked {
    fn from(mut state: IdentityState) -> Self {
        state
            .fields
            .iter_mut()
            .find(|field| {
                matches!(
                    field,
                    IdentityField::Email {
                        value: _,
                        first_challenge: _,
                        second_challenge: _
                    }
                )
            })
            .map(|field| match field {
                IdentityField::Email {
                    value: _,
                    first_challenge: _,
                    second_challenge,
                } => {
                    *second_challenge = None;
                }
                _ => panic!(),
            });

        IdentityStateBlanked { inner: state }
    }
}

// TODO: Doc
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityStateBlanked {
    #[serde(flatten)]
    inner: IdentityState,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityState {
    pub context: IdentityContext,
    pub is_fully_verified: bool,
    pub inserted_timestamp: Timestamp,
    pub completion_timestamp: Option<Timestamp>,
    pub judgement_submitted: bool,
    pub issue_judgement_at: Option<Timestamp>,
    pub fields: Vec<IdentityField>,
}

impl IdentityState {
    pub fn new(context: IdentityContext, fields: Vec<RawFieldValue>) -> Self {
        IdentityState {
            context,
            is_fully_verified: false,
            inserted_timestamp: Timestamp::now(),
            completion_timestamp: None,
            judgement_submitted: false,
            issue_judgement_at: None,
            // TODO
            //fields: fields.into_iter().map(IdentityField::new).collect(),
            fields: vec![],
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityContext {
    pub address: IdentityAddress,
    pub chain: Network,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityAddress(String);

impl IdentityAddress {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for IdentityAddress {
    fn from(v: String) -> Self {
        IdentityAddress(v)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Network(String);

impl Network {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityField {
    LegalName {
        value: String,
        manually_verified: bool,
    },
    DisplayName {
        value: String,
        passed_check: bool,
    },
    Email {
        value: String,
        first_challenge: Challenge,
        // IMPORTANT: Must be blanked before sending to user. See
        // `IdentityFieldBlanked`, respectively the `impl From<IdentityField>
        // for IdentityFieldBlanked` implementation.
        second_challenge: Option<Challenge>,
    },
    Web {
        value: String,
        manually_verified: bool,
    },
    Twitter {
        value: String,
        challenge: Challenge,
    },
    Matrix {
        value: String,
        challenge: Challenge,
    },
    PGPFingerprint,
    Image,
    Additional,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Challenge {
    pub value: String,
    pub is_verified: bool,
}

impl Challenge {
    pub fn random() -> Self {
        use rand::{thread_rng, Rng};

        let random: [u8; 16] = thread_rng().gen();
        Challenge {
            value: hex::encode(random),
            is_verified: false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
// TODO: Rename
pub enum RawFieldValue {
    LegalName(String),
    DisplayName(String),
    Email(String),
    Web(String),
    Twitter(String),
    Matrix(String),
    PGPFingerprint(()),
    Image(()),
    Additional(()),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalMessage {
    pub origin: ExternalMessageType,
    pub id: ExternalMessageId,
    pub timestamp: Timestamp,
    pub values: Vec<ExternalMessagePart>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalMessageId(u64);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ExternalMessageType {
    Email(String),
    Twitter(String),
    Matrix(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Timestamp(u64);

impl Timestamp {
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let start = SystemTime::now();
        let time = start
            .duration_since(UNIX_EPOCH)
            .expect("Failed to calculate UNIX time")
            .as_secs();

        Timestamp(time)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalMessagePart(String);

impl From<String> for ExternalMessagePart {
    fn from(val: String) -> Self {
        ExternalMessagePart(val)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Event {
    pub timestamp: Timestamp,
    pub event: NotificationMessage,
}

impl Event {
    pub fn new(event: NotificationMessage) -> Self {
        Event {
            timestamp: Timestamp::now(),
            event,
        }
    }
}

impl From<NotificationMessage> for Event {
    fn from(val: NotificationMessage) -> Self {
        Event::new(val)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum NotificationMessage {
    IdentityInserted {
        context: IdentityContext,
    },
    IdentityUpdated {
        context: IdentityContext,
    },
    FieldVerified {
        context: IdentityContext,
        field: RawFieldValue,
    },
    FieldVerificationFailed {
        context: IdentityContext,
        field: RawFieldValue,
    },
    SecondFieldVerified {
        context: IdentityContext,
        field: RawFieldValue,
    },
    SecondFieldVerificationFailed {
        context: IdentityContext,
        field: RawFieldValue,
    },
    AwaitingSecondChallenge {
        context: IdentityContext,
        field: RawFieldValue,
    },
    IdentityFullyVerified {
        context: IdentityContext,
    },
    JudgementProvided {
        context: IdentityContext,
    },
    ManuallyVerified {
        context: IdentityContext,
        field: RawFieldName,
    },
}

impl NotificationMessage {
    pub fn context(&self) -> &IdentityContext {
        use NotificationMessage::*;

        match self {
            IdentityInserted { context } => context,
            IdentityUpdated { context } => context,
            FieldVerified { context, field: _ } => context,
            FieldVerificationFailed { context, field: _ } => context,
            SecondFieldVerified { context, field: _ } => context,
            SecondFieldVerificationFailed { context, field: _ } => context,
            AwaitingSecondChallenge { context, field: _ } => context,
            IdentityFullyVerified { context } => context,
            JudgementProvided { context } => context,
            ManuallyVerified { context, field: _ } => context,
        }
    }
}
