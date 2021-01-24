use eventually::Aggregate;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct IdentityState<'a> {
    identities: HashMap<IdentityAddress, Vec<FieldStatus>>,
    lookup_addresses: HashMap<&'a IdentityField, HashSet<&'a IdentityAddress>>,
}

// TODO: Should logs be printed if users are not found?
impl<'a> IdentityState<'a> {
    fn new() -> Self {
        IdentityState {
            identities: HashMap::new(),
            lookup_addresses: HashMap::new(),
        }
    }
    fn insert_identity(&'a mut self, identity: IdentityInfo) {
        // Insert identity.
        let (address, fields) = (identity.address, identity.fields);
        self.identities.insert(address.clone(), fields);

        // Acquire references to the key/value from within the map. Unwrapping
        // is fine here since the value was just inserted.
        let (address, fields) = self.identities.get_key_value(&address).unwrap();

        // Create fast lookup tables.
        for field in fields {
            self.lookup_addresses
                .entry(&field.field)
                .and_modify(|active_addresses| {
                    active_addresses.insert(address);
                })
                .or_insert(vec![address].into_iter().collect());
        }
    }
    fn lookup_field_status(
        &self,
        address: &IdentityAddress,
        field: &IdentityField,
    ) -> Option<&FieldStatus> {
        self.identities
            .get(address)
            .map(|statuses| statuses.iter().find(|status| &status.field == field))
            // Unpack `Option<Option<T>>` to `Option<T>`
            .and_then(|status| status)
    }
    fn lookup_field_status_mut(
        &mut self,
        address: &IdentityAddress,
        field: &IdentityField,
    ) -> Option<&mut FieldStatus> {
        self.identities
            .get_mut(address)
            .map(|statuses| statuses.iter_mut().find(|status| &status.field == field))
            // Unpack `Option<Option<T>>` to `Option<T>`
            .and_then(|status| status)
    }
    // Lookup all addresses which contain the specified field.
    fn lookup_addresses(&self, field: &IdentityField) -> Option<Vec<&IdentityAddress>> {
        self.lookup_addresses
            .get(field)
            .map(|addresses| addresses.iter().map(|address| *address).collect())
    }
    pub fn verify_message(
        &'a self,
        field: &IdentityField,
        message: &ProvidedMessage,
    ) -> Vec<VerificationOutcome<'a>> {
        let mut outcomes = vec![];

        // Lookup all addresses which contain the field.
        if let Some(addresses) = self.lookup_addresses(field) {
            // For each address, verify the field.
            for address in addresses {
                if let Some(field_status) = self.lookup_field_status(&address, field) {
                    // Only verify if it has not been already.
                    if field_status.is_verified {
                        continue;
                    }

                    // Track address if the expected message was found.
                    outcomes.push(
                        if let Some(message_part) = field_status.expected_message.contains(message)
                        {
                            VerificationOutcome {
                                address: address,
                                expected_message: &field_status.expected_message,
                                status: VerificationStatus::Valid,
                            }
                        } else {
                            VerificationOutcome {
                                address: address,
                                expected_message: &field_status.expected_message,
                                status: VerificationStatus::Invalid,
                            }
                        },
                    );
                }
            }
        };

        outcomes
    }
    // TODO: Should return Result
    pub fn set_verified(&mut self, address: &IdentityAddress, field: &IdentityField) -> bool {
        if let Some(field_status) = self.lookup_field_status_mut(address, field) {
            field_status.is_verified = true;
            true
        } else {
            false
        }
    }
    // TODO: Should return Result
    pub fn is_fully_verified(&self, address: &IdentityAddress) -> Option<bool> {
        self.identities
            .get(address)
            .map(|field_statuses| field_statuses.iter().any(|status| status.is_verified))
    }
    // TODO: Should return Result
    pub fn remove_identity(&mut self, address: &IdentityAddress) -> bool {
        false
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct VerificationOutcome<'a> {
    pub address: &'a IdentityAddress,
    pub expected_message: &'a ExpectedMessage,
    pub status: VerificationStatus,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub enum VerificationStatus {
    Valid,
    Invalid,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityInfo {
    address: IdentityAddress,
    fields: Vec<FieldStatus>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct IdentityAddress(String);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityChallenge(Vec<u32>);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentitySignature(Vec<u32>);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct FieldStatus {
    field: IdentityField,
    expected_message: ExpectedMessage,
    is_verified: bool,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct FieldAddress(String);

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ExpectedMessage(String);

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ProvidedMessage {
    parts: Vec<ProvidedMessagePart>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ProvidedMessagePart(String);

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
pub enum IdentityField {
    #[serde(rename = "legal_name")]
    LegalName(FieldAddress),
    #[serde(rename = "display_name")]
    DisplayName(FieldAddress),
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
