#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityContext {
    pub chain_address: ChainAddress,
    pub chain_name: ChainName,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChainAddress(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainName {
    Polkadot,
    Kusama,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChainRemark {
    pub context: IdentityContext,
    pub timestamp: Timestamp,
    pub remark: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityField {
    pub value: IdentityFieldValue,
    pub expected_challenge: ExpectedChallenge,
    pub is_verified: bool,
    pub failed_attempts: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExpectedChallenge(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum IdentityFieldValue {
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
pub struct JudgementState {
    pub context: IdentityContext,
    pub fields: Vec<IdentityField>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalMessage {
    #[serde(rename = "type")]
    pub origin: ExternalMessageType,
    pub id: MessageId,
    pub timestamp: Timestamp,
    values: Vec<MessagePart>,
}

impl ExternalMessage {
    pub fn contains_challenge(&self, challenge: &ExpectedChallenge) -> bool {
        for value in &self.values {
            if value.0.contains(&challenge.0) {
                return true;
            }
        }

        false
    }
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
pub struct MessageId(u64);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Timestamp(u64);

impl Timestamp {
    pub fn now() -> Self {
        Timestamp(0)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessagePart(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Notification {
    #[serde(rename = "type")]
    ty: NotificationType,
    message: NotificationMessage,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Event {
    timestamp: Timestamp,
    event: NotificationMessage,
}

impl From<NotificationMessage> for Event {
    fn from(val: NotificationMessage) -> Self {
        Event {
            // TODO:
            timestamp: Timestamp(0),
            event: val,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum NotificationMessage {
    NoJudgementRequest,
    FieldVerified(IdentityFieldValue),
    VerificationFailed(IdentityFieldValue),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityJudged {
    context: IdentityContext,
    timestamp: Timestamp,
}
