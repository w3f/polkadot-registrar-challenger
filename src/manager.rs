use crate::aggregate::display_name::DisplayNameHandler;
use crate::api::start_api;
use crate::event::{
    BlankNetwork, Event, EventType, FieldStatusVerified, IdentityInserted, Notification,
};
use crate::{Error, Result};
use eventually::Aggregate;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;

pub enum UpdateChanges {
    VerificationValid(IdentityField),
    VerificationInvalid(IdentityField),
    BackAndForthExpected(IdentityField),
}

impl From<UpdateChanges> for Notification {
    fn from(val: UpdateChanges) -> Self {
        match val {
            UpdateChanges::VerificationValid(field) => {
                Notification::Success(format!("The {} field has been verified", field))
            }
            UpdateChanges::VerificationInvalid(field) => {
                Notification::Warn(format!("The {} field has failed verification", field))
            }
            UpdateChanges::BackAndForthExpected(field) => Notification::Info(format!(
                "The first challenge of the {0} field has been verified. \
                An additional challenge has been sent to {0}",
                field
            )),
        }
    }
}

#[derive(Default)]
pub struct IdentityManager<'a> {
    identities: HashMap<NetworkAddress, Vec<FieldStatus>>,
    lookup_addresses: HashMap<IdentityField, HashSet<NetworkAddress>>,
    display_names: Vec<DisplayName>,
    _p1: std::marker::PhantomData<&'a ()>,
}

// TODO: Should logs be printed if users are not found?
impl<'a> IdentityManager<'a> {
    fn new() -> Self {
        IdentityManager {
            identities: HashMap::new(),
            lookup_addresses: HashMap::new(),
            display_names: Vec::new(),
            _p1: std::marker::PhantomData,
        }
    }
    pub fn insert_identity(&mut self, identity: IdentityInserted) {
        // Take value from Event wrapper.
        let identity = identity.identity;

        // Insert identity.
        let (net_address, fields) = (identity.net_address, identity.fields);
        self.identities.insert(net_address.clone(), fields);

        // Acquire references to the key/value from within the map. Unwrapping
        // is fine here since the value was just inserted.
        let (net_address, fields) = self.identities.get_key_value(&net_address).unwrap();

        // Create fast lookup tables.
        for field in fields {
            self.lookup_addresses
                .entry(field.field.clone())
                .and_modify(|active_addresses| {
                    active_addresses.insert(net_address.clone());
                })
                .or_insert(vec![net_address].into_iter().cloned().collect());
        }
    }
    pub fn update_field(&mut self, verified: FieldStatusVerified) -> Result<Option<UpdateChanges>> {
        self.identities
            .get_mut(&verified.net_address)
            .ok_or(anyhow!("network address not found"))
            .and_then(|statuses| {
                statuses
                    .iter_mut()
                    .find(|status| status.field == verified.field_status.field)
                    .map(|status| {
                        let verified_status = verified.field_status;

                        // If the current field status has already been
                        // verified, skip (and avoid sending a new
                        // notification).
                        if (status.is_valid() && verified_status.is_valid())
                            || (status.is_valid() && verified_status.is_not_valid())
                        {
                            None
                        }
                        // Return a warning that no changes have been committed
                        // since the verification is invalid.
                        else if status.is_not_valid() && verified_status.is_not_valid() {
                            Some(UpdateChanges::VerificationInvalid(verified_status.field))
                        }
                        // Verification is valid, so commit changes. Gernerate
                        // different notifications based on the individual
                        // challenge type.
                        else if status.is_not_valid() && verified_status.is_valid() {
                            let field = verified_status.field.clone();
                            *status = verified_status;

                            match &status.challenge {
                                ChallengeStatus::ExpectMessage(_)
                                | ChallengeStatus::CheckDisplayName(_) => {
                                    Some(UpdateChanges::VerificationValid(field))
                                }
                                ChallengeStatus::BackAndForth(challenge) => {
                                    // The first message has been verified, now
                                    // the second message must be verified.
                                    if challenge.first_check_status == Validity::Valid
                                        && challenge.second_check_status == Validity::Invalid
                                    {
                                        Some(UpdateChanges::VerificationValid(field))
                                    }
                                    // Both messages have been fully verified.
                                    else if challenge.first_check_status == Validity::Valid
                                        && challenge.second_check_status == Validity::Valid
                                    {
                                        Some(UpdateChanges::BackAndForthExpected(field))
                                    }
                                    // This case should never occur. Better safe than sorry.
                                    else {
                                        None
                                    }
                                }
                            }
                        }
                        // This case should never occur. Better safe than sorry.
                        else {
                            None
                        }
                    })
                    .ok_or(anyhow!("field not found"))
            })
    }
    fn lookup_field_status(
        &self,
        net_address: &NetworkAddress,
        field: &IdentityField,
    ) -> Option<&FieldStatus> {
        self.identities
            .get(net_address)
            .map(|statuses| statuses.iter().find(|status| &status.field == field))
            // Unpack `Option<Option<T>>` to `Option<T>`
            .and_then(|status| status)
    }
    // Lookup all addresses which contain the specified field.
    fn lookup_addresses(&self, field: &IdentityField) -> Option<Vec<&NetworkAddress>> {
        self.lookup_addresses
            .get(field)
            .map(|addresses| addresses.iter().map(|address| address).collect())
    }
    pub fn lookup_full_state(&self, net_address: &NetworkAddress) -> Option<IdentityState> {
        self.identities
            .get(net_address)
            .map(|fields| IdentityState {
                net_address: net_address.clone(),
                fields: fields.clone(),
            })
    }
    pub fn verify_display_name(
        &self,
        net_address: NetworkAddress,
        display_name: DisplayName,
    ) -> Result<Option<VerificationOutcome>> {
        let mut field_status = self
            .lookup_field_status(
                &net_address,
                &IdentityField::DisplayName(display_name.clone()),
            )
            .ok_or(anyhow!(
                "no identity found based on display name: \"{}\"",
                display_name.as_str()
            ))?
            .clone();

        let mut challenge = match &field_status.challenge {
            ChallengeStatus::CheckDisplayName(challenge) => {
                if challenge.status == Validity::Valid {
                    // The display name was already verified. Ignore.
                    return Ok(None);
                }

                challenge.clone()
            }
            _ => {
                return Err(anyhow!(
                    "expected to verify display name, found different challenge type"
                ))
            }
        };

        let handler = DisplayNameHandler::with_state(&self.display_names);
        let violations = handler.verify_display_name(&display_name);

        let outcome = if violations.is_empty() {
            VerificationOutcome {
                net_address: net_address,
                field_status: {
                    challenge.status = Validity::Valid;
                    challenge.similarities = Some(violations);
                    field_status.challenge = ChallengeStatus::CheckDisplayName(challenge);
                    field_status
                },
            }
        } else {
            VerificationOutcome {
                net_address: net_address,
                field_status: field_status,
            }
        };

        Ok(Some(outcome))
    }
    // TODO: This should return `Result<>`
    pub fn verify_message(
        &self,
        field: &IdentityField,
        provided_message: &ProvidedMessage,
    ) -> Option<VerificationOutcome> {
        /// Convenience function for verifying the message.
        // TODO: This does not actually set the challenge as valid/invalid...
        fn generate_outcome(
            net_address: NetworkAddress,
            field_status: FieldStatus,
            expected_message: &ExpectedMessage,
            provided_message: &ProvidedMessage,
        ) -> VerificationOutcome {
            if expected_message.contains(provided_message).is_some() {
                VerificationOutcome {
                    net_address: net_address,
                    field_status: field_status,
                }
            } else {
                VerificationOutcome {
                    net_address: net_address,
                    field_status: field_status,
                }
            }
        }

        // Lookup all addresses which contain the field.
        if let Some(net_addresses) = self.lookup_addresses(field) {
            // For each address, verify the field.
            for net_address in net_addresses {
                if let Some(field_status) = self.lookup_field_status(&net_address, field) {
                    // Variables must be cloned, since those are later converted
                    // into events (which require ownership) and sent to the
                    // event store.
                    let c_net_address = net_address.clone();
                    let mut c_field_status = field_status.clone();

                    // Verify the message, each verified specifically based on
                    // the challenge type.
                    match &field_status.challenge {
                        ChallengeStatus::ExpectMessage(challenge) => {
                            if challenge.status != Validity::Valid {
                                let outcome = if challenge
                                    .expected_message
                                    .contains(&provided_message)
                                    .is_some()
                                {
                                    VerificationOutcome {
                                        net_address: c_net_address,
                                        field_status: {
                                            // Clone the current state and overwrite the validity as `Valid`.
                                            let mut challenge = challenge.clone();
                                            challenge.status = Validity::Valid;
                                            c_field_status.challenge =
                                                ChallengeStatus::ExpectMessage(challenge);
                                            c_field_status
                                        },
                                    }
                                } else {
                                    VerificationOutcome {
                                        net_address: c_net_address,
                                        // Leave current state as is.
                                        field_status: c_field_status,
                                    }
                                };

                                return Some(outcome);
                            }
                        }
                        ChallengeStatus::BackAndForth(challenge) => {
                            // The first check must be verified before it can
                            // proceed on the seconds check.
                            let outcome = if challenge.first_check_status != Validity::Valid {
                                VerificationOutcome {
                                    net_address: c_net_address,
                                    field_status: {
                                        // Clone the current state and overwrite
                                        // the validity of the **first** status
                                        // as `Valid`.
                                        let mut challenge = challenge.clone();
                                        challenge.first_check_status = Validity::Valid;
                                        c_field_status.challenge =
                                            ChallengeStatus::BackAndForth(challenge);
                                        c_field_status
                                    },
                                }
                            } else if challenge.second_check_status != Validity::Valid {
                                VerificationOutcome {
                                    net_address: c_net_address,
                                    field_status: {
                                        // Clone the current state and overwrite
                                        // the validity of the **second** status
                                        // as `Valid`.
                                        let mut challenge = challenge.clone();
                                        challenge.second_check_status = Validity::Valid;
                                        c_field_status.challenge =
                                            ChallengeStatus::BackAndForth(challenge);
                                        c_field_status
                                    },
                                }
                            } else {
                                VerificationOutcome {
                                    net_address: c_net_address,
                                    // Leave current state as is.
                                    field_status: c_field_status,
                                }
                            };

                            return Some(outcome);
                        }
                        ChallengeStatus::CheckDisplayName(_) => {
                            error!("Received a display name check in the message verifier. This is a bug");
                        }
                    }
                }
            }
        };

        None
    }
    pub fn is_fully_verified(&self, net_address: &NetworkAddress) -> Result<bool> {
        self.identities
            .get(net_address)
            .map(|field_statuses| field_statuses.iter().any(|status| status.is_valid()))
            .ok_or(anyhow!(
                "failed to check the full verification status of unknown target: {:?}. This is a bug",
                net_address
            ))
    }
    // TODO: Should return Result
    pub fn remove_identity(&mut self, net_address: &NetworkAddress) -> bool {
        false
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct VerificationOutcome {
    pub net_address: NetworkAddress,
    pub field_status: FieldStatus,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "network", content = "address")]
pub enum NetworkAddress {
    #[serde(rename = "polkadot")]
    Polkadot(IdentityAddress),
    #[serde(rename = "kusama")]
    Kusama(IdentityAddress),
}

impl NetworkAddress {
    pub fn from(network: BlankNetwork, address: IdentityAddress) -> Self {
        match network {
            BlankNetwork::Polkadot => NetworkAddress::Polkadot(address),
            BlankNetwork::Kusama => NetworkAddress::Kusama(address),
        }
    }
    pub fn net_address_str(&self) -> &str {
        match self {
            NetworkAddress::Polkadot(address) => address.0.as_str(),
            NetworkAddress::Kusama(address) => address.0.as_str(),
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct IdentityState {
    pub net_address: NetworkAddress,
    fields: Vec<FieldStatus>,
}

impl From<IdentityState> for Event {
    fn from(val: IdentityState) -> Self {
        EventType::IdentityState(val).into()
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct IdentityAddress(String);

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct FieldStatus {
    field: IdentityField,
    is_permitted: bool,
    challenge: ChallengeStatus,
}

impl FieldStatus {
    pub fn is_valid(&self) -> bool {
        let status = match &self.challenge {
            ChallengeStatus::ExpectMessage(state) => &state.status,
            ChallengeStatus::BackAndForth(state) => {
                if state.first_check_status == Validity::Valid
                    && state.second_check_status == Validity::Valid
                {
                    return true;
                } else {
                    return false;
                }
            }
            ChallengeStatus::CheckDisplayName(state) => &state.status,
        };

        match status {
            Validity::Valid => true,
            Validity::Invalid | Validity::Unconfirmed => false,
        }
    }
    /// Convenience method for improved readability.
    pub fn is_not_valid(&self) -> bool {
        !self.is_valid()
    }
}

// TODO: Maybe rename to `ChallengeType`?
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "state")]
pub enum ChallengeStatus {
    #[serde(rename = "expect_message")]
    ExpectMessage(ExpectMessageChallenge),
    #[serde(rename = "back_and_forth")]
    BackAndForth(BackAndForthChallenge),
    #[serde(rename = "display_name_check")]
    CheckDisplayName(CheckDisplayNameChallenge),
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ExpectMessageChallenge {
    pub expected_message: ExpectedMessage,
    pub from: IdentityField,
    pub to: IdentityField,
    pub status: Validity,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct BackAndForthChallenge {
    pub expected_message: ExpectedMessage,
    // VERY IMPORTANT: This field MUST be skipped during serializing and MAY NO
    // be sent to the the end user via the API, since the message must be
    // explicitly received by the specified `from` address and sent back to the
    // service (`to` address).
    #[serde(skip_serializing)]
    pub expected_message_back: ExpectedMessage,
    pub from: IdentityField,
    pub to: IdentityField,
    pub first_check_status: Validity,
    pub second_check_status: Validity,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct CheckDisplayNameChallenge {
    pub status: Validity,
    pub similarities: Option<Vec<DisplayName>>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum Validity {
    #[serde(rename = "valid")]
    Valid,
    #[serde(rename = "invalid")]
    Invalid,
    #[serde(rename = "unconfirmed")]
    Unconfirmed,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct DisplayName(String);

impl DisplayName {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct FieldAddress(String);

impl FieldAddress {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for FieldAddress {
    fn from(val: String) -> Self {
        FieldAddress(val)
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ExpectedMessage(String);

// TODO: Should be moved to `crate::events`
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ProvidedMessage {
    pub parts: Vec<ProvidedMessagePart>,
}

// TODO: Should be moved to `crate::events`
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ProvidedMessagePart(String);

impl From<String> for ProvidedMessagePart {
    fn from(val: String) -> Self {
        ProvidedMessagePart(val)
    }
}

impl ExpectedMessage {
    fn contains<'a>(&self, message: &'a ProvidedMessage) -> Option<&'a ProvidedMessagePart> {
        for part in &message.parts {
            if self.0.contains(&part.0) {
                return Some(part);
            }
        }

        None
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "address")]
pub enum IdentityField {
    #[serde(rename = "legal_name")]
    LegalName(FieldAddress),
    #[serde(rename = "display_name")]
    DisplayName(DisplayName),
    #[serde(rename = "email")]
    Email(FieldAddress),
    #[serde(rename = "web")]
    Web(FieldAddress),
    #[serde(rename = "twitter")]
    Twitter(FieldAddress),
    #[serde(rename = "matrix")]
    Matrix(FieldAddress),
    #[serde(rename = "pgpFingerprint")]
    PGPFingerprint(FieldAddress),
    #[serde(rename = "image")]
    /// NOTE: Currently unsupported.
    Image,
    #[serde(rename = "additional")]
    /// NOTE: Currently unsupported.
    Additional,
}

impl fmt::Display for IdentityField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let string = match self {
            IdentityField::LegalName(addr) => format!("legal name (\"{}\")", addr.as_str()),
            IdentityField::DisplayName(addr) => format!("display name (\"{}\")", addr.as_str()),
            IdentityField::Email(addr) => format!("email (\"{}\")", addr.as_str()),
            IdentityField::Web(addr) => format!("web (\"{}\")", addr.as_str()),
            IdentityField::Twitter(addr) => format!("twitter (\"{}\")", addr.as_str()),
            IdentityField::Matrix(addr) => format!("matrix (\"{}\")", addr.as_str()),
            IdentityField::PGPFingerprint(addr) => {
                format!("PGP Fingerprint: (\"{}\")", addr.as_str())
            }
            IdentityField::Image => format!("image"),
            IdentityField::Additional => format!("additional information"),
        };

        write!(f, "{}", string)
    }
}

#[test]
fn print_identity_info() {
    let state = IdentityState {
        net_address: NetworkAddress::Polkadot(IdentityAddress(
            "15MUBwP6dyVw5CXF9PjSSv7SdXQuDSwjX86v1kBodCSWVR7cw".to_string(),
        )),
        fields: vec![
            FieldStatus {
                field: IdentityField::Matrix(FieldAddress("@alice:matrix.org".to_string())),
                challenge: ChallengeStatus::ExpectMessage(ExpectMessageChallenge {
                    expected_message: ExpectedMessage("1127233905".to_string()),
                    from: IdentityField::Matrix(FieldAddress("@alice:matrix.org".to_string())),
                    to: IdentityField::Matrix(FieldAddress("@registrar:matrix.org".to_string())),
                    status: Validity::Valid,
                }),
            },
            FieldStatus {
                field: IdentityField::Email(FieldAddress("alice@email.com".to_string())),
                challenge: ChallengeStatus::BackAndForth(BackAndForthChallenge {
                    expected_message: ExpectedMessage("6861321088".to_string()),
                    from: IdentityField::Email(FieldAddress("alice@email.com".to_string())),
                    to: IdentityField::Email(FieldAddress("registrar@web3.foundation".to_string())),
                    first_check_status: Validity::Unconfirmed,
                    second_check_status: Validity::Unconfirmed,
                }),
            },
            FieldStatus {
                field: IdentityField::DisplayName(FieldAddress("Alice in Wonderland".to_string())),
                challenge: ChallengeStatus::CheckDisplayName(CheckDisplayNameChallenge {
                    status: Validity::Valid,
                    similarities: None,
                }),
            },
        ],
    };

    use crate::event::{Notification, StateWrapper};

    println!(
        "{}",
        serde_json::to_string(&StateWrapper {
            state: state,
            notifications: vec![Notification::Success(
                "The Matrix account has been verified".to_string()
            ),]
        })
        .unwrap()
    )
}
