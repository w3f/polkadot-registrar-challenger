use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::{
    ExpectedMessage, FieldAddress, IdentityAddress, IdentityField, IdentityInfo, NetworkAddress,
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
pub struct Timestamp(u128);

impl Timestamp {
    pub fn unix_time() -> Self {
        let start = SystemTime::now();
        let unix_time = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();

        Timestamp(unix_time)
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct TTL(u128);

impl TTL {
    pub fn from_secs(secs: u64) -> Self {
        TTL((secs * 1_000) as u128)
    }
    pub fn immortal() -> Self {
        TTL(0)
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct EventHeader {
    event_name: EventName,
    timestamp: Timestamp,
    ttl: TTL,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum EventName {
    #[serde(rename = "full_state_request")]
    FullStateRequest,
    #[serde(rename = "identity_info")]
    IdentityInfo,
    #[serde(rename = "full_state_not_found_response")]
    FullStateNotFoundResponse,
}

impl From<IdentityInfo> for Event<IdentityInfo> {
    fn from(val: IdentityInfo) -> Self {
        Event {
            header: EventHeader {
                event_name: EventName::IdentityInfo,
                timestamp: Timestamp::unix_time(),
                ttl: TTL::immortal(),
            },
            body: val,
        }
    }
}

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
            ExternalOrigin::Email => IdentityField::Email(address),
            ExternalOrigin::Matrix => IdentityField::Matrix(address),
            ExternalOrigin::Twitter => IdentityField::Twitter(address),
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct FullStateRequest {
    pub net_address: NetworkAddress,
}

impl From<FullStateRequest> for Event<FullStateRequest> {
    fn from(val: FullStateRequest) -> Self {
        Event {
            header: EventHeader {
                event_name: EventName::FullStateRequest,
                timestamp: Timestamp::unix_time(),
                ttl: TTL::immortal(),
            },
            body: val,
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct FullStateNotFoundResponse {
    pub net_address: NetworkAddress,
}

impl From<FullStateNotFoundResponse> for Event<FullStateNotFoundResponse> {
    fn from(val: FullStateNotFoundResponse) -> Self {
        Event {
            header: EventHeader {
                event_name: EventName::FullStateNotFoundResponse,
                timestamp: Timestamp::unix_time(),
                ttl: TTL::immortal(),
            },
            body: val,
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct StateWrapper {
    #[serde(flatten)]
    pub state: IdentityInfo,
    pub notifications: Vec<Notification>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "level", content = "message")]
pub enum Notification {
    Success(String),
    Info(String),
    Warn(String),
    Error(String),
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlankNetwork {
    Polkadot,
    Kusama,
}
