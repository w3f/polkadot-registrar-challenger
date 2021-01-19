use eventually::Aggregate;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct IdentityState<'a> {
    identities: HashMap<IdentityAddress, Vec<FieldStatus>>,
    lookup_addresses: HashMap<&'a IdentityField, HashSet<&'a IdentityAddress>>,
}

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
    // Lookup all addresses which contain the specified field (*owned*). This
    // method is used to get around Rust's borrow checker.
    fn lookup_addresses_owned(&self, field: &IdentityField) -> Option<Vec<IdentityAddress>> {
        self.lookup_addresses
            .get(field)
            .map(|addresses| addresses.iter().map(|address| *address).cloned().collect())
    }
    fn is_fully_verified(&self, address: &IdentityAddress) -> Option<bool> {
        self.identities
            .get(address)
            .map(|field_statuses| field_statuses.iter().any(|status| status.is_verified))
    }
    fn verify_message(
        &mut self,
        field: &IdentityField,
        message: &ExpectedMessage,
    ) -> Vec<IdentityAddress> {
        // TODO: Log if None?
        let mut address_changes = vec![];

        // Lookup all addresses which contain the field.
        if let Some(addresses) = self.lookup_addresses_owned(field) {
            // For each address, verify the field.
            for address in addresses {
                if let Some(field_status) = self.lookup_field_status_mut(&address, field) {
                    // Set as verified if valid.
                    if field_status.expected_message.contains(message) {
                        address_changes.push(address);
                        field_status.is_verified = true;
                    }
                }
            }
        };

        address_changes
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityInfo {
    address: IdentityAddress,
    fields: Vec<FieldStatus>,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityAddress(String);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct IdentityPubkey(Vec<u32>);

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

impl ExpectedMessage {
    fn contains(&self, message: &ExpectedMessage) -> bool {
        self.0.contains(&message.0)
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
