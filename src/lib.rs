#[macro_use]
extern crate serde;

use std::cell::Cell;

mod adapters;

#[derive(Serialize, Deserialize)]
struct PubKey;
#[derive(Serialize, Deserialize)]
struct Challenge;

#[derive(Eq, PartialEq, Serialize, Deserialize)]
struct Address;

#[derive(Eq, PartialEq, Serialize, Deserialize)]
enum AddressType {
    Email(Address),
    Web(Address),
    Twitter(Address),
    Riot(Address),
}

#[derive(Serialize, Deserialize)]
struct OnChainIdentity {
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
    fn address_state(&self, addr_type: &AddressType) -> &AddressState {
        match addr_type {
            AddressType::Email(_) => &self.email,
            AddressType::Web(_) => &self.web,
            AddressType::Twitter(_) => &self.twitter,
            AddressType::Riot(_) => &self.riot,
        }
    }
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
    pub_key: PubKey,
    challenge: Challenge,
    confirmed: Cell<bool>,
}

impl AddressState {
    fn verify_challenge(&self) -> bool {
        // If valid...
        let valid = false;
        if valid {
            // Update db
            self.confirmed.set(true);
            true
        } else {
            false
        }
    }
}

struct IdentityManager {
    idents: Vec<OnChainIdentity>,
}

impl IdentityManager {
    pub fn new() -> Self {
        IdentityManager { idents: vec![] }
    }
    pub fn register_request(&mut self, ident: OnChainIdentity) {
        self.idents.push(ident);
    }
    pub fn get_identity_scope(&self, addr_type: AddressType) -> Option<&AddressState> {
        self.idents
            .iter()
            .find(|ident| ident.address_state_match(&addr_type).is_some())
            .map(|ident| ident.address_state(&addr_type))
    }
}
