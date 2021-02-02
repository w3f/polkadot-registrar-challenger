use crate::api::SubId;
use crate::manager::{
    ExpectedMessage, FieldAddress, FieldStatus, IdentityAddress, IdentityField, IdentityState,
    NetworkAddress, ProvidedMessage,
};
use crate::Result;
use eventually_event_store_db::GenericEvent;
use matrix_sdk::api::r0::sync::sync_events::State;
use std::convert::TryFrom;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    header: EventHeader,
    pub body: EventType,
}

impl TryFrom<Event> for GenericEvent {
    type Error = anyhow::Error;

    fn try_from(val: Event) -> Result<Self> {
        GenericEvent::serialize(val).map_err(|err| err.into())
    }
}

impl Event {
    pub fn expect_identity_info(self) -> Result<IdentityState> {
        match self.body {
            EventType::IdentityState(val) => Ok(val),
            _ => Err(anyhow!("unexpected event type")),
        }
    }
    pub fn expect_external_message(self) -> Result<ExternalMessage> {
        match self.body {
            EventType::ExternalMessage(val) => Ok(val),
            _ => Err(anyhow!("unexpected event type")),
        }
    }
    pub fn expect_field_status_verified(self) -> Result<FieldStatusVerified> {
        match self.body {
            EventType::FieldStatusVerified(val) => Ok(val),
            _ => Err(anyhow!("unexpected event type")),
        }
    }
    pub fn expect_field_status_verified_ref(&self) -> Result<&FieldStatusVerified> {
        match &self.body {
            EventType::FieldStatusVerified(val) => Ok(val),
            _ => Err(anyhow!("unexpected event type")),
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum EventType {
    #[serde(rename = "full_state_request")]
    FullStateRequest(FullStateRequest),
    #[serde(rename = "identity_info")]
    IdentityState(IdentityState),
    #[serde(rename = "error_message")]
    ErrorMessage(ErrorMessage),
    #[serde(rename = "external_message")]
    ExternalMessage(ExternalMessage),
    #[serde(rename = "field_status_verified")]
    FieldStatusVerified(FieldStatusVerified),
    #[serde(rename = "identity_fully_verified")]
    IdentityFullyVerified(IdentityFullyVerified),
    #[serde(rename = "identity_inserted")]
    IdentityInserted(IdentityInserted),
}

impl From<EventType> for Event {
    fn from(val: EventType) -> Self {
        Event {
            header: EventHeader {
                timestamp: Timestamp::unix_time(),
                ttl: TTL::immortal(),
            },
            body: val,
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: u32,
    pub message: String,
}

impl From<ErrorMessage> for Event {
    fn from(val: ErrorMessage) -> Self {
        EventType::ErrorMessage(val).into()
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
    timestamp: Timestamp,
    ttl: TTL,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
// It's possible that a message is split into multiple chunks. For example,
// parsing an email might result in multiple messages (up to the parser).
pub struct ExternalMessage {
    pub origin: ExternalOrigin,
    pub field_address: FieldAddress,
    pub message: ProvidedMessage,
}

impl From<ExternalMessage> for Event {
    fn from(val: ExternalMessage) -> Self {
        EventType::ExternalMessage(val).into()
    }
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
// TODO: Delete, no longer required.
pub struct FullStateRequest {
    pub requester: SubId,
    pub net_address: NetworkAddress,
}

impl From<FullStateRequest> for Event {
    fn from(val: FullStateRequest) -> Self {
        EventType::FullStateRequest(val).into()
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct StateWrapper {
    #[serde(flatten)]
    pub state: IdentityState,
    pub notifications: Vec<Notification>,
}

impl StateWrapper {
    pub fn with_notifications(state: IdentityState, notifications: Vec<Notification>) -> Self {
        StateWrapper {
            state: state,
            notifications: notifications,
        }
    }
}

impl From<IdentityState> for StateWrapper {
    fn from(val: IdentityState) -> Self {
        StateWrapper {
            state: val,
            notifications: vec![],
        }
    }
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

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct FieldStatusVerified {
    pub net_address: NetworkAddress,
    pub field_status: FieldStatus,
}

impl From<FieldStatusVerified> for Event {
    fn from(val: FieldStatusVerified) -> Self {
        EventType::FieldStatusVerified(val).into()
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct IdentityFullyVerified {
    pub net_address: NetworkAddress,
}

impl From<IdentityFullyVerified> for Event {
    fn from(val: IdentityFullyVerified) -> Self {
        EventType::IdentityFullyVerified(val).into()
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityInserted {
    #[serde(flatten)]
    pub identity: IdentityState,
}

impl From<IdentityInserted> for Event {
    fn from(val: IdentityInserted) -> Self {
        EventType::IdentityInserted(val).into()
    }
}
