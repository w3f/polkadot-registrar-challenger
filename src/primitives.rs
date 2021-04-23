#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub struct ChainAddress(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ChainName {
    Polkadot,
    Kusama,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub struct ChainRemark(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
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
#[serde(rename_all="snake_case")]
pub struct JudgementState {
    pub chain_address: ChainAddress,
    pub chain_name: ChainName,
    pub fields: Vec<IdentityField>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub struct ExternalMessage {
    ty: ExternalMessageType,
    id: u64,
    origin: ExternalMessageOrigin,
    values: Vec<MessagePart>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub struct ExternalMessageOrigin(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ExternalMessageType {
    Email,
    Twitter,
    Matrix,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub struct MessagePart(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub struct Notification {
    ty: NotificationType,
    message: NotificationMessage,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum NotificationType {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum NotificationMessage {
    NoJudgementRequest,
    FieldVerified(IdentityFieldType)
}
