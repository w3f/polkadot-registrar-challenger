use super::{Address, AddressType, Challenge, PubKey, Result, Signature};
use crate::db::{Database, ScopedDatabase};
use crossbeam::channel::{unbounded, Receiver, Sender};
use failure::err_msg;
use std::collections::HashMap;
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
    pub matrix: Option<AddressState>,
}

impl OnChainIdentity {
    // Get the address state based on the address type (Email, Matrix, etc.).
    fn address_state(&self, addr_type: &AddressType) -> Option<&AddressState> {
        use AddressType::*;

        match addr_type {
            Email => self.email.as_ref(),
            Web => self.web.as_ref(),
            Twitter => self.twitter.as_ref(),
            Matrix => self.matrix.as_ref(),
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
    challenge: Challenge,
    confirmed: AtomicBool,
}

impl AddressState {
    pub fn new(addr: Address, addr_type: AddressType) -> Self {
        AddressState {
            addr: addr,
            addr_type: addr_type,
            addr_validity: AddressValidity::Unknown,
            challenge: Challenge::gen_random(),
            confirmed: AtomicBool::new(false),
        }
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

pub enum CommsMessage {
    NewOnChainIdentity(OnChainIdentity),
    Inform {
        context: AddressContext,
        challenge: Challenge,
    },
    ValidAddress {
        context: AddressContext,
    },
    InvalidAddress {
        context: AddressContext,
    },
}

pub struct AddressContext {
    pub pub_key: PubKey,
    pub address: Address,
    pub address_ty: AddressType,
}

pub struct CommsMain {
    sender: Sender<CommsMessage>,
    // TODO: This can be removed.
    address_ty: AddressType,
}

// TODO: Avoid clones
impl CommsMain {
    fn inform(&self, pub_key: &PubKey, address: &Address, challenge: &Challenge) {
        self.sender
            .send(CommsMessage::Inform {
                context: AddressContext {
                    pub_key: pub_key.clone(),
                    address: address.clone(),
                    address_ty: self.address_ty.clone(),
                },
                challenge: challenge.clone(),
            })
            .unwrap();
    }
}

pub struct CommsVerifier {
    tx: Sender<CommsMessage>,
    recv: Receiver<CommsMessage>,
    address_ty: AddressType,
}

// TODO: Avoid clones
impl CommsVerifier {
    pub fn recv(&self) -> CommsMessage {
        self.recv.recv().unwrap()
    }
    /// Receive a `Inform` message. This is only used by the Matrix client as
    /// any other message type will panic.
    pub fn recv_inform(&self) -> (AddressContext, Challenge) {
        if let CommsMessage::Inform { context, challenge} = self.recv() {
            (context, challenge)
        } else {
            panic!("received invalid message type on Matrix client");
        }
    }
    pub fn valid_feedback(&self, pub_key: &PubKey, addr: &Address) {
        self.tx
            .send(CommsMessage::ValidAddress {
                context: AddressContext {
                    pub_key: pub_key.clone(),
                    address: addr.clone(),
                    address_ty: self.address_ty.clone(),
                }
            })
            .unwrap();
    }
    pub fn invalid_feedback(&self, pub_key: &PubKey, addr: &Address) {
        self.tx
            .send(CommsMessage::InvalidAddress {
                context: AddressContext {
                    pub_key: pub_key.clone(),
                    address: addr.clone(),
                    address_ty: self.address_ty.clone(),
                }
            })
            .unwrap();
    }
}

pub struct IdentityManager<'a> {
    pub idents: Vec<OnChainIdentity>,
    pub db: ScopedDatabase<'a>,
    comms: CommsTable,
}

struct CommsTable {
    to_main: Sender<CommsMessage>,
    listener: Receiver<CommsMessage>,
    pairs: HashMap<AddressType, CommsMain>,
}

impl<'a> IdentityManager<'a> {
    pub fn new(db: &'a Database) -> Result<Self> {
        let db = db.scope("pending_identities");

        let mut idents = vec![];

        // Read pending on-chain identities from storage. Ideally, there are none.
        for (_, value) in db.all()? {
            idents.push(OnChainIdentity::from_json(&*value)?);
        }

        let (tx, recv) = unbounded();

        Ok(IdentityManager {
            idents: idents,
            db: db,
            comms: CommsTable {
                to_main: tx,
                listener: recv,
                pairs: HashMap::new(),
            },
        })
    }
    pub fn register_comms(&'static mut self, addr_type: AddressType) -> CommsVerifier {
        let (tx, recv) = unbounded();

        self.comms.pairs.insert(
            addr_type.clone(),
            CommsMain {
                sender: tx,
                address_ty: addr_type.clone(),
            },
        );

        CommsVerifier {
            tx: self.comms.to_main.clone(),
            recv: recv,
            address_ty: addr_type,
        }
    }
    pub async fn run(&mut self) -> Result<()> {
        use CommsMessage::*;

        if let Ok(msg) = self.comms.listener.recv() {
            match msg {
                CommsMessage::NewOnChainIdentity(ident) => {
                    self.register_request(ident)?;
                }
                CommsMessage::Inform { .. } => {
                    // INVALID
                    // TODO: log
                }
                ValidAddress {
                    context
                } => {}
                InvalidAddress {
                    context
                } => {}
            }
        }

        Ok(())
    }
    pub fn register_request(&mut self, ident: OnChainIdentity) -> Result<()> {
        // Only add the identity to the list if it doesn't exists yet.
        if self
            .idents
            .iter()
            .find(|ident| ident.pub_key == ident.pub_key)
            .is_none()
        {
            // Save the pending on-chain identity to disk.
            self.db.put(ident.pub_key.0.to_bytes(), ident.to_json()?)?;
            self.idents.push(ident);
        }

        let ident = self
            .idents
            .last()
            .ok_or(err_msg("last registered identity not found."))?;

        // TODO: Handle additional address types.
        ident.matrix.as_ref().map(|state| {
            self.comms.pairs.get(&state.addr_type).unwrap().inform(
                &ident.pub_key,
                &state.addr,
                &state.challenge,
            );
        });

        Ok(())
    }
}
