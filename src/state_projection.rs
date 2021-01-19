use eventually::Aggregate;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct IdentityState<'a> {
    identities: HashMap<IdentityAddress, Identity>,
    lookup_addresses: HashMap<&'a IdentityField, HashSet<&'a IdentityAddress>>,
}

impl<'a> IdentityState<'a> {
    pub fn new() -> Self {
        IdentityState {
            identities: HashMap::new(),
            lookup_addresses: HashMap::new(),
        }
    }
    pub fn insert_identity(&'a mut self, identity: Identity) {
        let address = identity.address.clone();

        self.identities.insert(address.clone(), identity);
        // Acquire references to the key/value from within the map. Unwrapping
        // is fine here since the value was just inserted.
        let (address, identity) = self.identities.get_key_value(&address).unwrap();

        for field in &identity.fields {
            self.lookup_addresses
                .entry(&field.field)
                .and_modify(|active_addresses| {
                    active_addresses.insert(address);
                })
                .or_insert(vec![address].into_iter().collect());
        }
    }
    pub fn lookup_addresses(&'a self, field: &IdentityField) -> Option<Vec<&'a IdentityAddress>> {
        self.lookup_addresses
            .get(field)
            .map(|addresses| addresses.iter().map(|address| *address).collect())
    }
    pub fn verify_signature(
        &'a mut self,
        field: &IdentityField,
        signature: &IdentitySignature,
    ) -> Option<&'a IdentityAddress> {
        if let Some(addresses) = self.lookup_addresses(field) {
            for address in addresses {
                if let Some(identity) = self.identities.get_mut(address) {
                    let pub_key = &identity.pub_key;

                    // TODO: Verify signature
                    let is_valid = false;
                    if is_valid {
                        // TODO: Log/error if `None`?
                        identity
                            .fields
                            .iter_mut()
                            .find(|status| &status.field == field)
                            .map(|mut status| status.is_verified = true);

                        return Some(&identity.address);
                    } else {
                        // TODO: Log?
                    }
                }
            }
        }

        None
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
pub struct IdentitySignature(Vec<u32>);

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct FieldStatus {
    field: IdentityField,
    challenge: IdentityChallenge,
    is_verified: bool,
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct FieldAddress(String);

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
