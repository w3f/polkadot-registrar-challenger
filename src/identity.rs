use crate::comms::{generate_comms, CommsMain, CommsMessage, CommsVerifier};
use crate::db::{Database, Database2};
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
    network_address: NetworkAddress,
    accounts: Vec<AccountState>,
}

#[derive(Debug, Fail)]
enum ManagerError {
    #[fail(display = "no handler registered for account type: {:?}", 0)]
    NoHandlerRegistered(AccountType),
}

impl OnChainIdentity {
    pub fn new(net_account: NetAccount) -> Result<Self> {
        Ok(OnChainIdentity {
            network_address: net_account.try_into()?,
            accounts: vec![],
        })
    }
    pub fn push_account(&mut self, account_ty: AccountType, account: Account) -> Result<()> {
        if self
            .accounts
            .iter()
            .find(|state| state.account_ty == account_ty)
            .is_some()
        {
            return Err(failure::err_msg(
                "cannot insert existing account types into identity",
            ));
        }

        self.accounts.push(AccountState::new(account, account_ty));
        Ok(())
    }
    pub fn net_account(&self) -> &NetAccount {
        self.network_address.address()
    }
    pub fn pub_key(&self) -> &PubKey {
        &self.network_address.pub_key()
    }
    pub fn get_account_state(&self, account_ty: &AccountType) -> Option<&AccountState> {
        self.accounts
            .iter()
            .find(|state| &state.account_ty == account_ty)
    }
    pub fn account_states(&self) -> &Vec<AccountState> {
        &self.accounts
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountState {
    pub account: Account,
    pub account_ty: AccountType,
    pub account_status: AccountStatus,
    pub challenge: Challenge,
    pub challenge_status: ChallengeStatus,
    pub skip_inform: bool,
}

impl AccountState {
    pub fn new(account: Account, account_ty: AccountType) -> Self {
        AccountState {
            account: account,
            account_ty: account_ty,
            account_status: AccountStatus::Unknown,
            challenge: Challenge::gen_random(),
            challenge_status: ChallengeStatus::Unconfirmed,
            skip_inform: false,
        }
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
    db2: Database2,
    comms: CommsTable,
}

struct CommsTable {
    to_main: Sender<CommsMessage>,
    listener: Receiver<CommsMessage>,
    pairs: HashMap<AccountType, CommsMain>,
}

impl IdentityManager {
    pub fn new(db: Database, db2: Database2) -> Result<Self> {
        let mut idents = HashMap::new();

        let (tx1, recv1) = unbounded();

        Ok(IdentityManager {
            idents: idents,
            db: db,
            db2: db2,
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
        // No async support for `recv` (it blocks and chokes tokio), so we
        // `try_recv` and just loop over it with a short pause.
        let mut interval = time::interval(Duration::from_millis(10));
        loop {
            interval.tick().await;
            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
            });
        }
    }
    pub async fn local(&mut self) -> Result<()> {
        use CommsMessage::*;

        if let Ok(msg) = self.comms.listener.try_recv() {
            match msg {
                NewJudgementRequest(ident) => {
                    debug!("Manager received a new judgement request. Forwarding");
                    self.db2.insert_identity(&ident).await?;

                    for state in ident.account_states() {
                        self.comms
                            .pairs
                            .get(&state.account_ty)
                            .ok_or(ManagerError::NoHandlerRegistered(state.account_ty.clone()))
                            .map(|comms| {
                                comms.notify_account_verification(
                                    ident.net_account().clone(),
                                    state.account.clone(),
                                )
                            })?;
                    }
                }
                _ => panic!("Received unrecognized message type. Report as a bug"),
            }
        }

        Ok(())
    }
}
