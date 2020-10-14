use crate::comms::{generate_comms, CommsMain, CommsMessage, CommsVerifier};
use crate::db::Database2;
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, Judgement, NetAccount, NetworkAddress, Result,
};
use crossbeam::channel::{unbounded, Receiver, Sender};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::collections::HashMap;
use std::convert::TryInto;
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

static WHITELIST: [AccountType; 4] = [
    AccountType::DisplayName,
    AccountType::Matrix,
    AccountType::Email,
    AccountType::Twitter,
];

static NOTIFY_QUEUE: [AccountType; 3] = [
    AccountType::Matrix,
    AccountType::Email,
    AccountType::Twitter,
];

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

impl FromSql for AccountStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(val) => match val {
                b"unknown" => Ok(AccountStatus::Unknown),
                b"valid" => Ok(AccountStatus::Valid),
                b"invalid" => Ok(AccountStatus::Invalid),
                b"notified" => Ok(AccountStatus::Notified),
                _ => Err(FromSqlError::InvalidType),
            },
            _ => Err(FromSqlError::InvalidType),
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
    async fn local(&mut self) -> Result<()> {
        use CommsMessage::*;

        if let Ok(msg) = self.comms.listener.try_recv() {
            match msg {
                NewJudgementRequest(ident) => self.handle_new_judgment_request(ident).await?,
                NotifyStatusChange { net_account } => {
                    self.handle_status_change(net_account).await?
                }
                MessageAcknowledged => {}
                _ => panic!("Received unrecognized message type. Report as a bug"),
            }
        }

        self.handle_verification_timeouts().await?;

        Ok(())
    }
    // TODO: Remove display_name
    async fn handle_verification_timeouts(&self) -> Result<()> {
        const TIMEOUT_LIMIT: u64 = 3600;

        let net_accounts = self.db2.select_timed_out_identities(TIMEOUT_LIMIT).await?;

        let connector_comms = self.get_comms(&AccountType::ReservedConnector)?;
        let matrix_comms = self.get_comms(&AccountType::Matrix)?;

        for net_account in net_accounts {
            info!("Deleting expired account: {}", net_account.as_str());
            connector_comms.notify_identity_judgment(net_account.clone(), Judgement::Erroneous);
            matrix_comms.leave_matrix_room(net_account);
        }

        self.db2.cleanup_timed_out_identities(TIMEOUT_LIMIT).await?;

        Ok(())
    }
    async fn handle_new_judgment_request(&mut self, mut ident: OnChainIdentity) -> Result<()> {
        debug!(
            "Handling new judgment request for account: {}",
            ident.net_account().as_str()
        );

        // Check the current, associated addresses of the identity, if any.
        let accounts = self.db2.select_addresses(&ident.net_account()).await?;
        let mut to_delete = vec![];

        // Find duplicates.
        for state in ident.account_states() {
            // Reject the entire judgment request if a non-white listed account type is specified.
            if !WHITELIST.contains(&state.account_ty) {
                warn!(
                    "Reject identity {}, use of unacceptable account type: {:?}",
                    ident.net_account().as_str(),
                    state.account_ty
                );

                self.get_comms(&AccountType::ReservedConnector)?
                    .notify_identity_judgment(ident.net_account().clone(), Judgement::Erroneous);

                return Ok(());
            }

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
    async fn handle_status_change(&mut self, net_account: NetAccount) -> Result<()> {
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
                comms.leave_matrix_room(net_account.clone());
            })?;

            // Leave enough time for the Matrix client to close the room.
            // Not really a problem if that does not happen.
            time::delay_for(Duration::from_secs(3)).await;

            self.db2.remove_identity(&net_account).await?;

            return Ok(());
        }

        // If an account is marked invalid, implying the account could not be
        // reached, then find any other valid accounts which can be contacted
        // and informed about the state of the invalid account. The user should
        // then update the on-chain identity. Additionally, there's a special
        // case of `display_name` which can be deemed invalid if it is too
        // similar to another, existing `display_name` in the identity system.

        /// Find a valid account of the identity which can be notified about an
        /// other account's invalidity. Preference for Matrix, since it's
        /// instant, followed by Email then Twitter.
        fn find_valid(
            account_statuses: &[(AccountType, Account, AccountStatus)],
        ) -> Option<&'static AccountType> {
            let filtered = account_statuses
                .iter()
                .filter(|(_, _, status)| status == &AccountStatus::Valid)
                .map(|(account_ty, _, _)| account_ty)
                .collect::<Vec<&AccountType>>();

            for to_notify in &NOTIFY_QUEUE {
                if filtered.contains(&to_notify) {
                    return Some(to_notify);
                }
            }

            None
        }

        /// Find invalid accounts.
        fn find_invalid<'a>(
            account_statuses: &[(AccountType, Account, AccountStatus)],
        ) -> Vec<(AccountType, Account)> {
            account_statuses
                .iter()
                .cloned()
                .filter(|(_, _, status)| status == &AccountStatus::Invalid)
                .map(|(account_ty, account, _)| (account_ty, account))
                .collect::<Vec<(AccountType, Account)>>()
        }

        let account_statuses = self.db2.select_account_statuses(&net_account).await?;

        // If invalid accounts were found, attempt to contact the user in order
        // to inform that person about the current state of invalid accounts.
        let invalid_accounts = find_invalid(&account_statuses);
        if !invalid_accounts.is_empty() {
            if let Some(to_notify) = find_valid(&account_statuses) {
                self.get_comms(to_notify)
                    .map(|comms| {
                        comms.notify_invalid_accounts(net_account.clone(), invalid_accounts);
                    })?;
            } else {
                warn!("Identity {} could not be informed about invalid accounts (no valid accounts yet)", net_account.as_str());
            }
        }

        Ok(())
    }
}
