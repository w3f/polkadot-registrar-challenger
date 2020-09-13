use super::{Address, AddressType, Challenge, PubKey, Signature};
use rocksdb::{IteratorMode, DB};
use schnorrkel::context::SigningContext;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Serialize, Deserialize)]
pub struct OnChainIdentity {
    pub_key: PubKey,
    // TODO: Should this just be a String?
    display_name: Option<AddressState>,
    // TODO: Should this just be a String?
    legal_name: Option<AddressState>,
    email: Option<AddressState>,
    web: Option<AddressState>,
    twitter: Option<AddressState>,
    riot: Option<AddressState>,
}

impl OnChainIdentity {
    // Get the address state based on the address type (Email, Riot, etc.).
    fn address_state(&self, addr_type: &AddressType) -> Option<&AddressState> {
        match addr_type {
            AddressType::Email => self.email.as_ref(),
            AddressType::Web => self.web.as_ref(),
            AddressType::Twitter => self.twitter.as_ref(),
            AddressType::Riot => self.riot.as_ref(),
        }
    }
    // Get the address state based on the addresses type. If the addresses
    // themselves match (`me@email.com == me@email.com`), it returns the state
    // wrapped in `Some(_)`, or `None` if the match is invalid.
    fn address_state_match(
        &self,
        addr_type: &AddressType,
        addr: &Address,
    ) -> Option<&AddressState> {
        if let Some(addr_state) = self.address_state(addr_type) {
            if &addr_state.addr == addr {
                return Some(addr_state);
            }
        }
        None
    }
    fn from_json(val: &[u8]) -> Self {
        serde_json::from_slice(&val).unwrap()
    }
    fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }
}

#[derive(Serialize, Deserialize)]
struct AddressState {
    addr: Address,
    addr_type: AddressType,
    addr_validity: AddressValidity,
    challenge: Challenge,
    confirmed: AtomicBool,
}

#[derive(Eq, PartialEq, Serialize, Deserialize)]
enum AddressValidity {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "valid")]
    Valid,
    #[serde(rename = "invalid")]
    Invalid,
}

pub struct IdentityScope<'a> {
    identity: &'a OnChainIdentity,
    addr_state: &'a AddressState,
    db: &'a DB,
}

impl<'a> IdentityScope<'a> {
    pub fn address(&self) -> &Address {
        &self.addr_state.addr
    }
    fn verify_challenge(&self, sig: Signature) -> bool {
        self.identity
            .pub_key
            .0
            // TODO: Check context in substrate.
            .verify_simple(b"", self.addr_state.challenge.0.as_bytes(), &sig.0)
            .and_then(|_| {
                self.addr_state.confirmed.store(true, Ordering::Relaxed);

                // Keep track of the current progress on disk.
                self.db
                    .put(self.identity.pub_key.0.to_bytes(), self.identity.to_json())
                    .unwrap();

                Ok(true)
            })
            .or_else::<(), _>(|_| Ok(false))
            .unwrap()
    }
}

pub struct IdentityManager {
    idents: Vec<OnChainIdentity>,
    db: DB,
}

impl<'a> IdentityManager {
    pub fn new(db: DB) -> Self {
        let mut idents = vec![];

        // Read pending on-chain identities from storage. Ideally, there are none.
        db.iterator(IteratorMode::Start).for_each(|(_, value)| {
            idents.push(OnChainIdentity::from_json(&*value));
        });

        IdentityManager {
            idents: idents,
            db: db,
        }
    }
    pub fn register_request(&mut self, ident: OnChainIdentity) {
        if self.pub_key_exists(&ident.pub_key) {
            return;
        }

        // Save the pending on-chain identity to disk.
        self.db.put(ident.pub_key.0.to_bytes(), ident.to_json());

        self.idents.push(ident);
    }
    fn pub_key_exists(&self, pub_key: &PubKey) -> bool {
        self.idents
            .iter()
            .find(|ident| &ident.pub_key == pub_key)
            .is_some()
    }
    pub fn get_identity_scope(
        &'a self,
        addr_type: &AddressType,
        addr: &Address,
    ) -> Option<IdentityScope<'a>> {
        self.idents
            .iter()
            .find(|ident| ident.address_state_match(addr_type, addr).is_some())
            .map(|ident| IdentityScope {
                identity: &ident,
                // Unwrapping is fine here, since `Some` is verified in the
                // previous `find()` combinator.
                addr_state: &ident.address_state(&addr_type).unwrap(),
                db: &self.db,
            })
    }
    pub fn get_uninitialized_channel(&'a self, addr_type: AddressType) -> Vec<IdentityScope<'a>> {
        self.idents
            .iter()
            .filter(|ident| {
                if let Some(addr_state) = ident.address_state(&addr_type) {
                    addr_state.addr_validity == AddressValidity::Unknown
                } else {
                    false
                }
            })
            .map(|ident| IdentityScope {
                identity: &ident,
                // Unwrapping is fine here, since `None` values are filtered out
                // in the previous `filter` combinator.
                addr_state: &ident.address_state(&addr_type).unwrap(),
                db: &self.db,
            })
            .collect()
    }
}
