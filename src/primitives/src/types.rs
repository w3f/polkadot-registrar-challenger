use serde::{Serialize, Deserialize};

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
            fields: fields.into_iter().map(IdentityField::new).collect(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityContext {
    pub address: ChainAddress,
    pub chain: ChainName,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChainAddress(String);

impl ChainAddress {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for ChainAddress {
    fn from(v: String) -> Self {
        ChainAddress(v)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainName {
    Polkadot,
    Kusama,
}

impl ChainName {
    pub fn as_str(&self) -> &str {
        match self {
            ChainName::Polkadot => "polkadot",
            ChainName::Kusama => "kusama",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityField {
    pub value: RawFieldValue,
    pub challenge: ChallengeType,
    // TODO: Change this to usize.
    pub failed_attempts: isize,
}

// TODO: Should be `From`?
impl IdentityField {
    pub fn new(val: RawFieldValue) -> Self {
        use RawFieldValue::*;

        let challenge = {
            match val {
                LegalName(_) => ChallengeType::Manual { is_verified: None },
                Web(_) => ChallengeType::Manual { is_verified: None },
                PGPFingerprint(_) => ChallengeType::Manual { is_verified: None },
                Image(_) => ChallengeType::Manual { is_verified: None },
                Additional(_) => ChallengeType::Manual { is_verified: None },
                DisplayName(_) => ChallengeType::DisplayNameCheck {
                    passed: false,
                    violations: vec![],
                },
                Email(_) => ChallengeType::ExpectedMessage {
                    expected: ExpectedMessage::random(),
                    second: Some(ExpectedMessage::random()),
                },
                Twitter(_) => ChallengeType::ExpectedMessage {
                    expected: ExpectedMessage::random(),
                    second: None,
                },
                Matrix(_) => ChallengeType::ExpectedMessage {
                    expected: ExpectedMessage::random(),
                    second: None,
                },
            }
        };

        IdentityField {
            value: val,
            challenge,
            failed_attempts: 0,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "content")]
pub enum ChallengeType {
    ExpectedMessage {
        expected: ExpectedMessage,
        second: Option<ExpectedMessage>,
    },
    DisplayNameCheck {
        passed: bool,
        violations: Vec<DisplayNameEntry>,
    },
    Manual {
        is_verified: Option<bool>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExpectedMessage {
    pub value: String,
    pub is_verified: bool,
}

impl ExpectedMessage {
    pub fn random() -> Self {
        use rand::{thread_rng, Rng};

        let random: [u8; 16] = thread_rng().gen();
        ExpectedMessage {
            value: hex::encode(random),
            is_verified: false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
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

impl RawFieldValue {
    // TODO: Rename
    pub fn matches(&self, message: &ExternalMessage) -> bool {
        match self {
            RawFieldValue::Email(n1) => match &message.origin {
                ExternalMessageType::Email(n2) => n1 == n2,
                _ => false,
            },
            RawFieldValue::Twitter(n1) => match &message.origin {
                ExternalMessageType::Twitter(n2) => n1 == n2,
                _ => false,
            },
            RawFieldValue::Matrix(n1) => match &message.origin {
                ExternalMessageType::Matrix(n2) => n1 == n2,
                _ => false,
            },
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalMessage {
    pub origin: ExternalMessageType,
    pub timestamp: Timestamp,
    pub values: Vec<ExternalMessagePart>,
}

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
