use super::{Address, AddressType, Challenge, PubKey, Result, Signature};
use crate::db::{Database, ScopedDatabase};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Serialize, Deserialize)]
pub struct OnChainIdentity {
    pub pub_key: PubKey,
    // TODO: Should this just be a String?
    pub display_name: Option<AddressState>,
    // TODO: Should this just be a String?
    pub legal_name: Option<AddressState>,
    pub email: Option<AddressState>,
    pub web: Option<AddressState>,
    pub twitter: Option<AddressState>,
    pub riot: Option<AddressState>,
}

impl OnChainIdentity {
    // Get the address state based on the address type (Email, Riot, etc.).
    fn address_state(&self, addr_type: &AddressType) -> Option<&AddressState> {
        use AddressType::*;

        match addr_type {
            Email => self.email.as_ref(),
            Web => self.web.as_ref(),
            Twitter => self.twitter.as_ref(),
            Riot => self.riot.as_ref(),
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
    fn from_json(val: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(&val)?)
    }
    fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

#[derive(Serialize, Deserialize)]
pub struct AddressState {
    addr: Address,
    addr_type: AddressType,
    addr_validity: AddressValidity,
    pub challenge: Challenge,
    pub attempt_contact: AtomicBool,
    confirmed: AtomicBool,
}

impl AddressState {
    pub fn new(addr: Address, addr_type: AddressType) -> Self {
        AddressState {
            addr: addr,
            addr_type: addr_type,
            addr_validity: AddressValidity::Unknown,
            challenge: Challenge::gen_random(),
            attempt_contact: AtomicBool::new(false),
            confirmed: AtomicBool::new(false),
        }
    }
    pub fn attempt_contact(&self) {
        self.attempt_contact.store(true, Ordering::Relaxed);
    }
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
    pub identity: &'a OnChainIdentity,
    pub addr_state: &'a AddressState,
    db: &'a ScopedDatabase<'a>,
}

impl<'a> IdentityScope<'a> {
    pub fn address(&self) -> &Address {
        &self.addr_state.addr
    }
    pub fn verify_challenge(&self, sig: Signature) -> Result<bool> {
        if let Ok(_) = self
            .identity
            .pub_key
            .0
            // TODO: Check context in substrate.
            .verify_simple(b"", self.addr_state.challenge.0.as_bytes(), &sig.0)
        {
            self.addr_state.confirmed.store(true, Ordering::Relaxed);

            // Keep track of the current progress on disk.
            self.db
                .put(self.identity.pub_key.0.to_bytes(), self.identity.to_json()?)?;

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub struct IdentityManager<'a> {
    pub idents: Vec<OnChainIdentity>,
    pub db: ScopedDatabase<'a>,
}

impl<'a> IdentityManager<'a> {
    pub fn new(db: &'a Database) -> Result<Self> {
        let db = db.scope("pending_identities");

        let mut idents = vec![];

        // Read pending on-chain identities from storage. Ideally, there are none.
        for (_, value) in db.all()? {
            idents.push(OnChainIdentity::from_json(&*value)?);
        };

        Ok(IdentityManager {
            idents: idents,
            db: db,
        })
    }
    pub fn register_request(&mut self, ident: OnChainIdentity) -> Result<()> {
        if !self.pub_key_exists(&ident.pub_key) {
            // Save the pending on-chain identity to disk.
            self.db.put(ident.pub_key.0.to_bytes(), ident.to_json()?)?;
            self.idents.push(ident);
        }

        Ok(())
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
                    addr_state.addr_validity == AddressValidity::Unknown && !addr_state.attempt_contact.load(Ordering::Relaxed)
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
