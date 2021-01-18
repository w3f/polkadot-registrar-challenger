use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type IdentityStateProjection<'a> = Arc<RwLock<IdentityState<'a>>>;

pub struct IdentityState<'a> {
    identities: HashMap<IdentityAddress, Identity>,
    identity_fields: Vec<&'a Vec<FieldStatus>>,
}

impl<'a> IdentityState<'a> {
    pub fn new() -> Self {
        IdentityState {
            identities: HashMap::new(),
            identity_fields: vec![],
        }
    }
    pub fn insert_identity(&'a mut self, identity: Identity) {
        let address = identity.address.clone();

        self.identities.insert(identity.address.clone(), identity);
        self.identity_fields
            // Unwrapping here is safe since the entry was just inserted
            .push(&self.identities.get(&address).unwrap().fields);
    }
    pub fn get_valid_verifications(&self, field: &IdentityField) {}
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
