use crate::adapters::email::EmailId;
use crate::manager::{
    DisplayName, FieldAddress, FieldStatus, IdentityField, IdentityState, NetworkAddress,
    OnChainChallenge, ProvidedMessage, UpdateChanges,
};
use crate::Result;
use std::convert::TryFrom;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    header: EventHeader,
    pub body: EventType,
}

impl TryFrom<eventstore::RecordedEvent> for Event {
    type Error = anyhow::Error;

    fn try_from(val: eventstore::RecordedEvent) -> Result<Self> {
        val.as_json::<Event>().map_err(|err| {
            anyhow!(
                "failed to deserialize 'RecordedEvent' to 'Event': {:?}",
                err
            )
        })
    }
}

impl TryFrom<Event> for eventstore::EventData {
    type Error = anyhow::Error;

    fn try_from(val: Event) -> Result<Self> {
        eventstore::EventData::json("registrar-event", val)
            .map_err(|err| anyhow!("failed to serialize 'Event' to 'EventData': {:?}", err))
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    IdentityInserted(IdentityInserted),
    ExternalMessage(ExternalMessage),
    FieldStatusVerified(FieldStatusVerified),
    IdentityFullyVerified(IdentityFullyVerified),
    DisplayNamePersisted(DisplayNamePersisted),
    ExportedIdentityState(Vec<IdentityState>),
    RemarkFound(RemarkFound),
    JudgementGiven(JudgementGiven),
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

// TODO: Move to api module?
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
pub enum ErrorMessage {
    NoPendingJudgementRequest(String),
}

impl ErrorMessage {
    pub fn no_pending_judgement_request(registrar_idx: usize) -> Self {
        ErrorMessage::NoPendingJudgementRequest(format!(
            "This identity does not have a pending judgement request for registrar #{}",
            registrar_idx
        ))
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
    Email(EmailId),
    // Matrix does not have a ID to track. This part is handled by the Matrix
    // SDK (which syncs a tracking token).
    #[serde(rename = "matrix")]
    Matrix,
    #[serde(rename = "twitter")]
    Twitter(()),
}

impl From<(ExternalOrigin, FieldAddress)> for IdentityField {
    fn from(val: (ExternalOrigin, FieldAddress)) -> Self {
        let (origin, address) = val;

        match origin {
            ExternalOrigin::Email(_) => IdentityField::Email(address),
            ExternalOrigin::Matrix => IdentityField::Matrix(address),
            ExternalOrigin::Twitter(_) => IdentityField::Twitter(address),
        }
    }
}

// TODO: Move to API mode?
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
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
    // Convenience method which creates a "newly inserted" notification for the user.
    pub fn newly_inserted_notification(inserted: IdentityInserted) -> Self {
        let net_address = inserted.identity.net_address.clone();
        StateWrapper {
            state: inserted.identity,
            notifications: vec![UpdateChanges::NewIdentityInserted(net_address).into()],
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
    pub on_chain_challenge: OnChainChallenge,
}

impl From<IdentityFullyVerified> for Event {
    fn from(val: IdentityFullyVerified) -> Self {
        EventType::IdentityFullyVerified(val).into()
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
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

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DisplayNamePersisted {
    pub net_address: NetworkAddress,
    pub display_name: DisplayName,
}

impl From<DisplayNamePersisted> for Event {
    fn from(val: DisplayNamePersisted) -> Self {
        EventType::DisplayNamePersisted(val).into()
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct OnChainRemark(String);

impl OnChainRemark {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct RemarkFound {
    pub net_address: NetworkAddress,
    pub remark: OnChainRemark,
}

impl RemarkFound {
    pub fn as_str(&self) -> &str {
        self.remark.0.as_str()
    }
}

impl From<RemarkFound> for Event {
    fn from(val: RemarkFound) -> Self {
        EventType::RemarkFound(val).into()
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct JudgementGiven {
    pub net_address: NetworkAddress,
}

#[cfg(test)]
/// This module just contains convenient functionality to initialize test data.
/// The actual tests are placed in `src/tests/`.
mod tests {
    use super::*;

    impl From<IdentityState> for IdentityInserted {
        fn from(val: IdentityState) -> Self {
            IdentityInserted { identity: val }
        }
    }
}
