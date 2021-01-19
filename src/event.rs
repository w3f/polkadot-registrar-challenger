use crate::projection::{
    ExpectedMessage, FieldAddress, IdentityAddress, IdentityChallenge, IdentityField,
    ProvidedMessage,
};

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct Event<Body> {
    header: EventHeader,
    body: Body,
}

impl<Body> Event<Body> {
    pub fn body_ref(&self) -> &Body {
        &self.body
    }
    pub fn body(self) -> Body {
        self.body
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct Timestamp(u64);

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct TTL(u64);

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct EventHeader {
    event_version: EventVersion,
    event_name: EventName,
    timestamp: Timestamp,
    ttl: TTL,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct EventVersion(u32);

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum EventName {}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
// It's possible that a message is split into multiple chunks. For example,
// parsing an email might result in multiple messages (up to the parser).
pub struct ExternalMessage {
    pub origin: ExternalOrigin,
    pub field_address: FieldAddress,
    pub message: ProvidedMessage,
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

impl From<(ExternalOrigin, FieldAddress)> for IdentityField {
    fn from(val: (ExternalOrigin, FieldAddress)) -> Self {
        let (origin, address) = val;

        match origin {
            Email => IdentityField::Email(address),
            Matrix => IdentityField::Matrix(address),
            Twitter => IdentityField::Twitter(address),
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct IdentityVerification {
    pub address: IdentityAddress,
    pub field: IdentityField,
    pub provided_message: ProvidedMessage,
    pub expected_message: ExpectedMessage,
    pub is_valid: bool,
    pub is_fully_verified: bool,
}
