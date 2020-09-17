use crate::comms::{generate_comms, CommsMain, CommsMessage, CommsVerifier};
use crate::db::Database;
use crate::primitives::{
    Account, AccountType, Algorithm, Challenge, ChallengeStatus, Fatal, NetAccount, NetworkAddress, PubKey, Result,
};
use crossbeam::channel::{unbounded, Receiver, Sender};
use std::collections::HashMap;
use std::convert::TryInto;
use tokio::time::{self, Duration};

// TODO: add cfg
pub struct TestClient {
    comms: CommsVerifier,
}

impl TestClient {
    pub fn new(comms: CommsVerifier) -> Self {
        TestClient { comms: comms }
    }
    pub fn gen_data(&self) {
        let sk = schnorrkel::keys::SecretKey::generate();
        let pk = sk.to_public();

        let ident = OnChainIdentity {
            network_address: NetAccount::from("136nXcbVseRqQHvu6iDraV2Qi9p4YCMBgdeDpDLLgmRjEqVb")
                .try_into()
                .unwrap(),
            display_name: None,
            legal_name: None,
            email: None,
            web: None,
            twitter: None,
            matrix: Some(AccountState::new(
                Account::from("@fabio:web3.foundation"),
                AccountType::Matrix,
            )),
        };

        let random = &ident.matrix.as_ref().unwrap().challenge;

        let sig = sk.sign_simple(b"substrate", &random.as_bytes(), &pk);
        println!("SIG: >> {}", hex::encode(&sig.to_bytes()));

        self.comms.notify_new_identity(ident);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnChainIdentity {
    pub network_address: NetworkAddress,
    pub display_name: Option<String>,
    pub legal_name: Option<String>,
    pub email: Option<AccountState>,
    pub web: Option<AccountState>,
    pub twitter: Option<AccountState>,
    pub matrix: Option<AccountState>,
}

impl OnChainIdentity {
    pub fn pub_key(&self) -> &PubKey {
        &self.network_address.pub_key()
    }
    fn set_validity(&mut self, account_ty: AccountType, account_validity: AccountValidity) {
        use AccountType::*;

        match account_ty {
            Matrix => {
                self.matrix.as_mut().fatal().account_validity = account_validity;
            }
            _ => {}
        }
    }
    fn set_challenge_status(&mut self, account_ty: AccountType, challenge_status: ChallengeStatus) {
        use AccountType::*;

        match account_ty {
            Matrix => {
                self.matrix.as_mut().fatal().challenge_status = challenge_status;
            }
            _ => {}
        }
    }
    fn from_json(val: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(&val)?)
    }
    fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountState {
    account: Account,
    account_ty: AccountType,
    account_validity: AccountValidity,
    challenge: Challenge,
    challenge_status: ChallengeStatus,
}

impl AccountState {
    pub fn new(account: Account, account_ty: AccountType) -> Self {
        AccountState {
            account: account,
            account_ty: account_ty,
            account_validity: AccountValidity::Unknown,
            challenge: Challenge::gen_random(),
            challenge_status: ChallengeStatus::Unconfirmed,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum AccountValidity {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "valid")]
    Valid,
    #[serde(rename = "invalid")]
    Invalid,
    #[serde(rename = "notified")]
    Notified,
}

pub struct IdentityManager {
    idents: HashMap<NetAccount, OnChainIdentity>,
    db: Database,
    comms: CommsTable,
}

struct CommsTable {
    to_main: Sender<CommsMessage>,
    listener: Receiver<CommsMessage>,
    pairs: HashMap<AccountType, CommsMain>,
}

impl IdentityManager {
    pub fn new(db: Database) -> Result<Self> {
        let mut idents = HashMap::new();

        // Read pending on-chain identities from storage. Ideally, there are none.
        let db_idents = db.scope("pending_identities");
        for (_, value) in db_idents.all()? {
            let ident = OnChainIdentity::from_json(&*value).fatal();
            idents.insert(ident.network_address.address().clone(), ident);
        }

        println!("PRELOADED: {:?}", idents);

        let (tx1, recv1) = unbounded();

        Ok(IdentityManager {
            idents: idents,
            db: db,
            comms: CommsTable {
                to_main: tx1.clone(),
                listener: recv1,
                pairs: HashMap::new(),
            },
        })
    }
    pub fn register_comms(&mut self, account_ty: AccountType) -> CommsVerifier {
        let (cm, cv) = generate_comms(self.comms.to_main.clone(), account_ty.clone());
        self.comms.pairs.insert(account_ty, cm);
        cv
    }
    pub async fn start(mut self) -> Result<()> {
        use CommsMessage::*;

        // No async support for `recv` (it blocks and chokes tokio), so we
        // `try_recv` and just loop over it with a short pause.
        let mut interval = time::interval(Duration::from_millis(10));
        loop {
            if let Ok(msg) = self.comms.listener.try_recv() {
                match msg {
                    CommsMessage::NewJudgementRequest(ident) => {
                        self.handle_register_request(ident)?;
                    }
                    ChallengeConfirmation {
                        network_address,
                        account_ty,
                        status
                    } => {
                        self.handle_challenge_feedback(
                            network_address,
                            account_ty,
                            status,
                        );
                    }
                    AccountConfirmation {
                        network_address,
                        account_ty,
                        account_validity
                    } => {
                        self.handle_validity_feedback(
                            network_address,
                            account_ty,
                            account_validity,
                        );
                    }
                    TrackRoomId { address, room_id } => {
                        let db_rooms = self.db.scope("matrix_rooms");
                        db_rooms.put(address.as_str(), room_id.as_bytes())?;
                    }
                    RequestAccountState {
                        account,
                        account_ty,
                    } => {
                        self.handle_account_state_request(account, account_ty);
                    }
                    _ => panic!("Received unrecognized message type. Report as a bug"),
                }
            } else {
                interval.tick().await;
            }
        }
    }
    fn handle_challenge_feedback(
        &mut self,
        network_address: NetworkAddress,
        account_ty: AccountType,
        challenge_status: ChallengeStatus,
    ) {
        // Set the challenge status of the account type.
        let ident = self
            .idents
            .get_mut(network_address.address())
            .map(|ident| {
                ident.set_challenge_status(account_ty, challenge_status);
                ident
            })
            .fatal();

        // Save that info to storage.
        let db_idents = self.db.scope("pending_identities");
        db_idents
            .put(
                ident.network_address.address().as_str(),
                ident.to_json().fatal(),
            )
            .fatal();
    }
    fn handle_validity_feedback(
        &mut self,
        network_address: NetworkAddress,
        account_ty: AccountType,
        account_validity: AccountValidity,
    ) {
        // Confirm the validity of the account type.
        let ident = self
            .idents
            .get_mut(network_address.address())
            .map(|ident| {
                ident.set_validity(account_ty, account_validity);
                ident
            })
            .fatal();

        // Save that info to storage.
        let db_idents = self.db.scope("pending_identities");
        db_idents
            .put(
                ident.network_address.address().as_str(),
                ident.to_json().fatal(),
            )
            .fatal();

        // TODO: Notify existing channels about invalidity.
    }
    fn handle_register_request(&mut self, ident: OnChainIdentity) -> Result<()> {
        let db_idents = self.db.scope("pending_identities");
        let db_rooms = self.db.scope("matrix_rooms");

        // Save the pending on-chain identity to disk.
        db_idents.put(ident.network_address.address().as_str(), ident.to_json()?)?;

        // Save the pending on-chain identity to memory.
        self.idents
            .insert(ident.network_address.address().clone(), ident.clone());

        // Only matrix supported for now.
        ident.matrix.as_ref().map::<(), _>(|state| {
            let room_id = if let Some(bytes) = db_rooms
                .get(ident.network_address.address().as_str())
                .fatal()
            {
                Some(std::str::from_utf8(&bytes).fatal().try_into().fatal())
            } else {
                None
            };

            self.comms.pairs.get(&state.account_ty).fatal().inform_task(
                ident.network_address.clone(),
                state.account.clone(),
                state.challenge.clone(),
                room_id,
            );
        });

        Ok(())
    }
    fn handle_account_state_request(&self, account: Account, account_ty: AccountType) {
        let ident = self
            .idents
            .iter()
            .find(|(_, ident)| match account_ty {
                AccountType::Matrix => {
                    if let Some(state) = ident.matrix.as_ref() {
                        state.account == account
                    } else {
                        false
                    }
                }
                _ => panic!("Unsupported"),
            })
            .map(|(_, ident)| ident);

        let comms = self.comms.pairs.get(&AccountType::ReservedEmitter).fatal();

        if let Some(ident) = ident {
            // Account state was checked in `find` combinator.
            let state = match account_ty {
                AccountType::Matrix => ident.matrix.as_ref().fatal(),
                _ => panic!("Unsupported"),
            };

            comms.inform_task(
                ident.network_address.clone(),
                state.account.clone(),
                state.challenge.clone(),
                None,
            );
        } else {
            // There is no pending request for the user.
            comms.invalid_request();
        }
    }
}
