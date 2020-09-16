use super::{Account, AccountType, Challenge, PubKey, Result, RoomId};
use crate::db::{Database, ScopedDatabase};
use crossbeam::channel::{unbounded, Receiver, Sender};
use failure::err_msg;
use std::collections::HashMap;
use std::convert::TryInto;
use tokio::time::{self, Duration};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnChainIdentity {
    pub pub_key: PubKey,
    // TODO: Should this just be a String?
    pub display_name: Option<AccountState>,
    // TODO: Should this just be a String?
    pub legal_name: Option<AccountState>,
    pub email: Option<AccountState>,
    pub web: Option<AccountState>,
    pub twitter: Option<AccountState>,
    pub matrix: Option<AccountState>,
}

impl OnChainIdentity {
    pub fn from_json(val: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(&val)?)
    }
    pub fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountState {
    addr: Account,
    addr_type: AccountType,
    addr_validity: AccountValidity,
    // TODO: remove pub
    pub challenge: Challenge,
    confirmed: bool,
}

impl AccountState {
    pub fn new(addr: Account, addr_type: AccountType) -> Self {
        AccountState {
            addr: addr,
            addr_type: addr_type,
            addr_validity: AccountValidity::Unknown,
            challenge: Challenge::gen_random(),
            confirmed: false,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
enum AccountValidity {
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
        context: AccountContext,
        challenge: Challenge,
        room_id: Option<RoomId>,
    },
    ValidAccount {
        context: AccountContext,
    },
    InvalidAccount {
        context: AccountContext,
    },
    RoomId {
        pub_key: PubKey,
        room_id: RoomId,
    },
    // TODO: add AccountType option
    RequestFromUserId(Account),
}

pub struct AccountContext {
    pub pub_key: PubKey,
    pub address: Account,
    pub address_ty: AccountType,
}

pub struct CommsMain {
    sender: Sender<CommsMessage>,
    // TODO: This can be removed.
    address_ty: AccountType,
}

// TODO: Avoid clones
impl CommsMain {
    fn inform(
        &self,
        pub_key: &PubKey,
        address: &Account,
        challenge: &Challenge,
        room_id: Option<RoomId>,
    ) {
        self.sender
            .send(CommsMessage::Inform {
                context: AccountContext {
                    pub_key: pub_key.clone(),
                    address: address.clone(),
                    address_ty: self.address_ty.clone(),
                },
                challenge: challenge.clone(),
                room_id,
            })
            .unwrap();
    }
}

#[derive(Clone)]
pub struct CommsVerifier {
    tx: Sender<CommsMessage>,
    recv: Receiver<CommsMessage>,
    address_ty: AccountType,
}

// TODO: Avoid clones
impl CommsVerifier {
    pub async fn recv(&self) -> CommsMessage {
        let mut interval = time::interval(Duration::from_millis(50));

        loop {
            if let Ok(msg) = self.recv.try_recv() {
                return msg;
            } else {
                interval.tick().await;
            }
        }
    }
    /// Receive a `Inform` message. This is only used by the Matrix client as
    /// any other message type will panic.
    pub async fn recv_inform(&self) -> (AccountContext, Challenge, Option<RoomId>) {
        if let CommsMessage::Inform {
            context,
            challenge,
            room_id,
        } = self.recv().await
        {
            (context, challenge, room_id)
        } else {
            panic!("received invalid message type on Matrix client");
        }
    }
    pub fn request_address_sate(&self, address: &Account) {
        self.tx
            .send(CommsMessage::RequestFromUserId(address.clone()))
            .unwrap();
    }
    pub fn new_on_chain_identity(&self, ident: &OnChainIdentity) {
        self.tx
            .send(CommsMessage::NewOnChainIdentity(ident.clone()))
            .unwrap();
    }
    pub fn valid_feedback(&self, pub_key: &PubKey, addr: &Account) {
        self.tx
            .send(CommsMessage::ValidAccount {
                context: AccountContext {
                    pub_key: pub_key.clone(),
                    address: addr.clone(),
                    address_ty: self.address_ty.clone(),
                },
            })
            .unwrap();
    }
    pub fn invalid_feedback(&self, pub_key: &PubKey, addr: &Account) {
        self.tx
            .send(CommsMessage::InvalidAccount {
                context: AccountContext {
                    pub_key: pub_key.clone(),
                    address: addr.clone(),
                    address_ty: self.address_ty.clone(),
                },
            })
            .unwrap();
    }
    pub fn track_room_id(&self, pub_key: &PubKey, room_id: &RoomId) {
        self.tx
            .send(CommsMessage::RoomId {
                pub_key: pub_key.clone(),
                room_id: room_id.clone(),
            })
            .unwrap();
    }
}

pub struct IdentityManager {
    idents: Vec<OnChainIdentity>,
    db: Database,
    comms: CommsTable,
}

struct CommsTable {
    to_main: Sender<CommsMessage>,
    listener: Receiver<CommsMessage>,
    to_emitter: CommsMain,
    from_emitter: CommsVerifier,
    pairs: HashMap<AccountType, CommsMain>,
}

// let db_rooms = db.scope("pending_identities");
impl IdentityManager {
    pub fn new(db: Database) -> Result<Self> {
        let mut idents = vec![];

        // Read pending on-chain identities from storage. Ideally, there are none.
        let db_idents = db.scope("pending_identities");
        for (_, value) in db_idents.all()? {
            idents.push(OnChainIdentity::from_json(&*value)?);
        }

        let (tx1, recv1) = unbounded();
        let (tx2, recv2) = unbounded();

        Ok(IdentityManager {
            idents: idents,
            db: db,
            comms: CommsTable {
                to_main: tx1.clone(),
                listener: recv1,
                to_emitter: CommsMain {
                    sender: tx2,
                    address_ty: AccountType::Web,
                },
                from_emitter: CommsVerifier {
                    tx: tx1,
                    recv: recv2,
                    address_ty: AccountType::Web,
                },
                pairs: HashMap::new(),
            },
        })
    }
    pub fn register_comms(&mut self, addr_type: AccountType) -> CommsVerifier {
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
    // TODO: Maybe return this directly rather than cloning?
    pub fn emitter_comms(&self) -> CommsVerifier {
        self.comms.from_emitter.clone()
    }
    pub async fn start(mut self) -> Result<()> {
        use CommsMessage::*;
        let mut interval = time::interval(Duration::from_millis(50));

        loop {
            if let Ok(msg) = self.comms.listener.try_recv() {
                match msg {
                    CommsMessage::NewOnChainIdentity(ident) => {
                        self.register_request(ident)?;
                    }
                    CommsMessage::Inform { .. } => {
                        // INVALID
                        // TODO: log
                    }
                    ValidAccount { context: _ } => {}
                    InvalidAccount { context: _ } => {}
                    RoomId { pub_key, room_id } => {
                        let db_rooms = self.db.scope("matrix_rooms");
                        db_rooms.put(pub_key.0, room_id.0.as_bytes())?;
                    }
                    RequestFromUserId(addr) => {
                        // Find the identity based on the corresponding Matrix UserId.
                        let ident = self
                            .idents
                            .iter()
                            .find(|ident| {
                                if let Some(state) = ident.matrix.as_ref() {
                                    state.addr == addr
                                } else {
                                    false
                                }
                            })
                            .unwrap();
                        // TODO: Handle that unwrap.

                        // Unwrapping is safe here, since it's guaranteed in the
                        // `find` filter.
                        let state = ident.matrix.as_ref().unwrap();

                        // TODO: Report back whether the identity was found.
                        self.comms.to_emitter.inform(
                            &ident.pub_key,
                            &state.addr,
                            &state.challenge,
                            None,
                        );
                    }
                }
            } else {
                interval.tick().await;
            }
        }

        Ok(())
    }
    fn register_request(&mut self, ident: OnChainIdentity) -> Result<()> {
        // TODO: Handle updates

        let db_idents = self.db.scope("pending_identities");
        let db_rooms = self.db.scope("matrix_rooms");

        // Save the pending on-chain identity to disk.
        db_idents.put(ident.pub_key.0.to_bytes(), ident.to_json()?)?;
        self.idents.push(ident);

        let ident = self
            .idents
            .last()
            // TODO: necessary?
            .ok_or(err_msg("last registered identity not found."))?;

        // TODO: Handle additional address types.
        ident.matrix.as_ref().map(|state| {
            let room_id = db_rooms
                .get(&ident.pub_key.0)
                .unwrap()
                .map(|bytes| bytes.try_into().unwrap());

            self.comms.pairs.get(&state.addr_type).unwrap().inform(
                &ident.pub_key,
                &state.addr,
                &state.challenge,
                room_id,
            );
        });

        Ok(())
    }
}
