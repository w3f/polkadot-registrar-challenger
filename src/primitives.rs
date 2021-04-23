#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainAddress(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChainName {
    Polkadot,
    Kusama,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdentityField {
    ty: IdentityFieldType,
    value: IdentityFieldValue,
    is_verified: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum IdentityFieldType {
    LegalName,
    DisplayName,
    Email,
    Web,
    Twitter,
    Matrix,
    PGPFingerprint,
    Image,
    Additional,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct IdentityFieldValue(String);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct JudgementRequest {
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
    FieldVerified(IdentityFieldType, IdentityFieldValue)
}
