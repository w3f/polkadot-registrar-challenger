use std::collections::HashMap;

pub struct StateProjection {}

impl StateProjection {
    pub fn new() -> Self {
        StateProjection {}
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct Identity {
    address: IdentityAddress,
    pub_key: IdentityPubkey,
    fields: Vec<FieldStatus>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityAddress(String);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityPubkey(Vec<u32>);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityChallenge(Vec<u32>);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct FieldStatus {
    field: IdentityField,
    challenge: IdentityChallenge,
    is_verified: bool,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum IdentityField {
    #[serde(rename = "legal_name")]
    LegalName(),
    #[serde(rename = "display_name")]
    DisplayName,
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "web")]
    Web,
    #[serde(rename = "twitter")]
    Twitter,
    #[serde(rename = "matrix")]
    Matrix,
    #[serde(rename = "pgpFingerprint")]
    PGPFingerprint,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "additional")]
    Additional,
    // Reserved types for internal communication.
    //
    // Websocket connection to Watcher
    ReservedConnector,
    // Matrix emitter which reacts on Matrix messages
    ReservedEmitter,
}
