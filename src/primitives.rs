#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainAddress(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChainName {
    Polkadot,
    Kusama,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainRemark(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdentityField {
    field: IdentityFieldType,
    is_verified: bool,
    has_failed_attempt: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case", tag="type", content="value")]
pub enum IdentityFieldType {
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
pub struct JudgementState {
    pub chain_address: ChainAddress,
    pub chain_name: ChainName,
    pub fields: Vec<IdentityField>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExternalMessage {
    source: ExternalMessageSource,
    values: Vec<MessagePart>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExternalMessageSource {
    Email,
    Twitter,
    Matrix,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessagePart(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    ty: NotificationType,
    message: NotificationMessage,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum NotificationType {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum NotificationMessage {
    NoJudgementRequest,
    FieldVerified(IdentityFieldType)
}
