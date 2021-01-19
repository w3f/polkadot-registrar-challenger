use crate::projection::{ExpectedMessage, IdentityAddress, IdentityChallenge, IdentityField, ProvidedMessage};

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct Event<Body> {
    header: EventHeader,
    body: Body,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct EventHeader {
    event_version: EventVersion,
    event_name: EventName,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct EventVersion(u32);

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum EventName {}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
// It's possible that a message is split into multiple chunks. For example,
// parsing an email might result in multiple messages (up to the parser).
pub struct ExternalMessage {
    origin: ExternalOrigin,
    message: Vec<ExpectedMessage>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum ExternalOrigin {
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "matrix")]
    Matrix,
    #[serde(rename = "twitter")]
    Twitter,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct IdentityVerification {
    addresses: Vec<IdentityAddress>,
    field: IdentityField,
    provided_message: ProvidedMessage,
    expected_message: ExpectedMessage,
    is_valid: bool,
    is_fully_verified: bool,
}
