#[macro_use]
extern crate serde;

use rand::{thread_rng, Rng};
use rocksdb::{IteratorMode, DB};
use schnorrkel::context::SigningContext;
use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::Cell;

mod adapters;

struct PubKey(SchnorrkelPubKey);
struct Signature(SchnorrkelSignature);
#[derive(Serialize, Deserialize)]
struct Challenge(String);
#[derive(Eq, PartialEq, Serialize, Deserialize)]
struct Address;

impl Challenge {
    fn gen_random() -> Challenge {
        let random: [u8; 16] = thread_rng().gen();
        Challenge(hex::encode(random))
    }
}

#[derive(Eq, PartialEq, Serialize, Deserialize)]
enum AddressType {
    Email(Address),
    Web(Address),
    Twitter(Address),
    Riot(Address),
}

#[derive(Serialize, Deserialize)]
struct OnChainIdentity {
    pub_key: PubKey,
    // TODO: Should this just be a String?
    display_name: AddressState,
    // TODO: Should this just be a String?
    legal_name: AddressState,
    email: AddressState,
    web: AddressState,
    twitter: AddressState,
    riot: AddressState,
}

impl OnChainIdentity {
    // Get the address state based on the address type (Email, Riot, etc.).
    fn address_state(&self, addr_type: &AddressType) -> &AddressState {
        match addr_type {
            AddressType::Email(_) => &self.email,
            AddressType::Web(_) => &self.web,
            AddressType::Twitter(_) => &self.twitter,
            AddressType::Riot(_) => &self.riot,
        }
    }
    // Get the address state based on the addresses type. If the addresses
    // themselves match (`me@email.com == me@email.com`), it returns the state
    // wrapped in `Some(_)`, or `None` if the match is invalid.
    fn address_state_match(&self, addr_type: &AddressType) -> Option<&AddressState> {
        let addr_state = self.address_state(addr_type);

        if &addr_state.addr_type == addr_type {
            Some(addr_state)
        } else {
            None
        }
    }
}

#[derive(Serialize, Deserialize)]
struct AddressState {
    addr_type: AddressType,
    challenge: Challenge,
    confirmed: Cell<bool>,
}

struct IdentityScope<'a> {
    identity: &'a OnChainIdentity,
    addr_state: &'a AddressState,
    db: &'a DB,
}

impl<'a> IdentityScope<'a> {
    fn verify_challenge(&self, sig: Signature) -> bool {
        self.identity
            .pub_key
            .0
            // TODO: Check context in substrate.
            .verify_simple(b"", self.addr_state.challenge.0.as_bytes(), &sig.0)
            .and_then(|_| {
                self.addr_state.confirmed.set(true);

                // Keep track of the current progress on disk.
                self.db
                    .put(
                        self.identity.pub_key.0.to_bytes(),
                        serde_json::to_vec(self.identity).unwrap(),
                    )
                    .unwrap();

                Ok(true)
            })
            .or_else::<(), _>(|_| Ok(false))
            .unwrap()
    }
}

struct IdentityManager {
    idents: Vec<OnChainIdentity>,
    db: DB,
}

impl IdentityManager {
    pub fn new(db: DB) -> Self {
        let mut idents = vec![];

        // Read pending on-chain identities from storage. Ideally, there are none.
        db.iterator(IteratorMode::Start).for_each(|(_, value)| {
            idents.push(serde_json::from_slice::<OnChainIdentity>(&value).unwrap());
        });

        IdentityManager {
            idents: idents,
            db: db,
        }
    }
    pub fn register_request(&mut self, ident: OnChainIdentity) {
        // Save the pending on-chain identity to disk.
        self.db.put(
            ident.pub_key.0.to_bytes(),
            serde_json::to_vec(&ident).unwrap(),
        );

        self.idents.push(ident);
    }
    pub fn get_identity_scope<'a>(&'a self, addr_type: AddressType) -> Option<IdentityScope<'a>> {
        self.idents
            .iter()
            .find(|ident| ident.address_state_match(&addr_type).is_some())
            .map(|ident| IdentityScope {
                identity: &ident,
                addr_state: &ident.address_state(&addr_type),
                db: &self.db,
            })
    }
}

impl Serialize for PubKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for PubKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = <String as Deserialize>::deserialize(deserializer)?;
        Ok(PubKey(
            SchnorrkelPubKey::from_bytes(
                &hex::decode(hex_str)
                    .map_err(|_| SerdeError::custom("failed to decode public key from hex"))?,
            )
            .map_err(|_| SerdeError::custom("failed creating public key from bytes"))?,
        ))
    }
}
