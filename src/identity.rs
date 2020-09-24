use crate::comms::{generate_comms, CommsMain, CommsMessage, CommsVerifier};
use crate::db::Database;
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, Fatal, Judgement, NetAccount, NetworkAddress,
    PubKey, Result,
};
use crossbeam::channel::{unbounded, Receiver, Sender};
use rusqlite::types::{ToSql, ToSqlOutput, ValueRef};
use std::collections::HashMap;
use std::convert::TryInto;
use tokio::time::{self, Duration};

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
    fn set_validity(&mut self, account_ty: AccountType, account_validity: AccountStatus) {
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
    fn is_fully_verified(&self) -> bool {
        self.matrix
            .as_ref()
            .map(|state| state.challenge_status == ChallengeStatus::Accepted)
            .unwrap_or(true)
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
    pub account: Account,
    pub account_ty: AccountType,
    pub account_validity: AccountStatus,
    pub challenge: Challenge,
    pub challenge_status: ChallengeStatus,
    pub skip_inform: bool,
}

impl AccountState {
    pub fn new(account: Account, account_ty: AccountType) -> Self {
        AccountState {
            account: account,
            account_ty: account_ty,
            account_validity: AccountStatus::Unknown,
            challenge: Challenge::gen_random(),
            challenge_status: ChallengeStatus::Unconfirmed,
            skip_inform: false,
        }
    }
    pub fn account_str(&self) -> &str {
        self.account.as_str()
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum AccountStatus {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "valid")]
    Valid,
    #[serde(rename = "invalid")]
    Invalid,
    #[serde(rename = "notified")]
    Notified,
}

impl ToSql for AccountStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        use AccountStatus::*;
        use ToSqlOutput::*;
        use ValueRef::*;

        match self {
            Unknown => Ok(Borrowed(Text(b"unknown"))),
            Valid => Ok(Borrowed(Text(b"valid"))),
            Invalid => Ok(Borrowed(Text(b"invalid"))),
            Notified => Ok(Borrowed(Text(b"notified"))),
        }
    }
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
    pub async fn start(mut self) {
        use CommsMessage::*;

        // No async support for `recv` (it blocks and chokes tokio), so we
        // `try_recv` and just loop over it with a short pause.
        let mut interval = time::interval(Duration::from_millis(10));
        loop {
            if let Ok(msg) = self.comms.listener.try_recv() {
                match msg {
                    NewJudgementRequest(ident) => {
                        debug!("Manager received a new judgement request. Forwarding");
                    }
                    _ => panic!("Received unrecognized message type. Report as a bug"),
                }
            } else {
                interval.tick().await;
            }
        }
    }
}
