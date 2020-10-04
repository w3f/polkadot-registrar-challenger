use crate::comms::{generate_comms, CommsMain, CommsMessage, CommsVerifier};
use crate::db::Database2;
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, Judgement, NetAccount, NetworkAddress,
    PubKey, Result,
};
use crossbeam::channel::{unbounded, Receiver, Sender};
use rusqlite::types::{ToSql, ToSqlOutput, ValueRef};
use std::collections::HashMap;
use std::convert::TryInto;
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OnChainIdentity {
    network_address: NetworkAddress,
    accounts: Vec<AccountState>,
}

#[derive(Debug, Fail)]
enum ManagerError {
    #[fail(display = "no handler registered for account type: {:?}", 0)]
    NoHandlerRegistered(AccountType),
    #[fail(display = "failed to find account state of identity")]
    NoAccountState,
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
    pub fn remove_account_state(&mut self, account_ty: &AccountType) -> Result<()> {
        let pos = self
            .accounts
            .iter()
            .position(|state| &state.account_ty == account_ty)
            .ok_or(ManagerError::NoAccountState)?;
        self.accounts.remove(pos);

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
    db2: Database2,
    comms: CommsTable,
}

struct CommsTable {
    to_main: Sender<CommsMessage>,
    listener: Receiver<CommsMessage>,
    pairs: HashMap<AccountType, CommsMain>,
}

impl IdentityManager {
    pub fn new(db2: Database2) -> Result<Self> {
        let (tx1, recv1) = unbounded();

        Ok(IdentityManager {
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
    fn get_comms(&self, account_ty: &AccountType) -> StdResult<&CommsMain, ManagerError> {
        self.comms
            .pairs
            .get(account_ty)
            .ok_or(ManagerError::NoHandlerRegistered(account_ty.clone()))
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
                    self.handle_new_judgment_request(ident).await?;
                }
                NotifyStatusChange { net_account } => {
                    self.handle_status_change(net_account).await?;
                }
                MessageAcknowledged => {}
                _ => panic!("Received unrecognized message type. Report as a bug"),
            }
        }

        self.handle_verification_timeouts().await?;

        Ok(())
    }
    pub async fn handle_verification_timeouts(&self) -> Result<()> {
        const TIMEOUT_LIMIT: u64 = 3600;

        let net_accounts = self.db2.select_timed_out_identities(TIMEOUT_LIMIT).await?;

        let connector_comms = self.get_comms(&AccountType::ReservedConnector)?;
        let matrix_comms = self.get_comms(&AccountType::Matrix)?;

        for net_account in net_accounts {
            connector_comms.notify_identity_judgment(net_account.clone(), Judgement::Erroneous);
            matrix_comms.leave_matrix_room(net_account);
        }

        self.db2.cleanup_timed_out_identities(TIMEOUT_LIMIT).await?;

        Ok(())
    }
    pub async fn handle_new_judgment_request(&mut self, mut ident: OnChainIdentity) -> Result<()> {
        debug!(
            "Handling new judgment request for account: {}",
            ident.net_account().as_str()
        );

        // Check the current, associated addresses of the identity, if any.
        let accounts = self.db2.select_addresses(&ident.net_account()).await?;
        let mut to_delete = vec![];

        // Find duplicates.
        for state in ident.account_states() {
            // If the same account already exists in storage, remove it (and avoid replacement).
            if accounts
                .iter()
                .find(|&account| account == &state.account)
                .is_some()
            {
                to_delete.push(state.account_ty.clone());
            }
        }

        for ty in to_delete {
            ident.remove_account_state(&ty)?;
        }

        // Insert identity into storage, notify tasks for verification.
        self.db2.insert_identity(&ident).await?;

        for state in ident.account_states() {
            self.get_comms(&state.account_ty).map(|comms| {
                comms
                    .notify_account_verification(ident.net_account().clone(), state.account.clone())
            })?;
        }

        Ok(())
    }
    pub async fn handle_status_change(&mut self, net_account: NetAccount) -> Result<()> {
        debug!(
            "Handling status change for account: {}",
            net_account.as_str()
        );

        if self.db2.is_fully_verified(&net_account).await? {
            self.get_comms(&AccountType::ReservedConnector)
                .map(|comms| {
                    info!(
                        "Address {} is fully verified. Notifying Watcher...",
                        net_account.as_str()
                    );
                    comms.notify_identity_judgment(net_account.clone(), Judgement::Reasonable);
                })?;

            self.get_comms(&AccountType::Matrix).map(|comms| {
                debug!("Closing Matrix room for {}", net_account.as_str());
                comms.leave_matrix_room(net_account);
            })?;
        }

        Ok(())
    }
}
