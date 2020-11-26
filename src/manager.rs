use crate::comms::{generate_comms, CommsMain, CommsMessage, CommsVerifier};
use crate::db::Database;
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, Judgement, NetAccount, NetworkAddress, Result,
};
use crossbeam::channel::{unbounded, Receiver, Sender};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::collections::HashMap;
use std::convert::TryInto;
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

/// Identity info fields which are currently allowed to be judged. If there is
/// any other field present, the identity is immediately rejected.
static WHITELIST: [AccountType; 4] = [
    AccountType::DisplayName,
    AccountType::Matrix,
    AccountType::Email,
    AccountType::Twitter,
];

/// The ordering of account types in which the user is informed about invalid
/// fields (or the display name is too similar to an existing one): first, try
/// Matrix, then Email, etc.
///
/// See `IdentityManager::handle_status_change` for more.
static NOTIFY_QUEUE: [AccountType; 3] = [
    AccountType::Matrix,
    AccountType::Email,
    AccountType::Twitter,
];

/// The on-chain identity itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OnChainIdentity {
    network_address: NetworkAddress,
    accounts: Vec<AccountState>,
}

#[derive(Debug, Fail)]
pub enum ManagerError {
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
            #[cfg(not(test))]
            challenge: Challenge::gen_random(),
            #[cfg(test)]
            challenge: Challenge::gen_fixed(),
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
    #[serde(rename = "unsupported")]
    Unsupported,
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
            Unsupported => Ok(Borrowed(Text(b"unsupported"))),
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
    db: Database,
    comms: CommsTable,
    config: IdentityManagerConfig,
}

pub struct IdentityManagerConfig {
    judgement_timeout_limit: u64,
}

impl Default for IdentityManagerConfig {
    fn default() -> Self {
        IdentityManagerConfig {
            judgement_timeout_limit: 28800, // 8h
        }
    }
}

struct CommsTable {
    // A Sender to the Manager. Cloned when registering new communication
    // channels (see `IdentityManager::register_comms()`).
    to_main: Sender<CommsMessage>,
    // Listener for incoming messages from tasks.
    listener: Receiver<CommsMessage>,
    // Lookup table for channels to the requested tasks.
    pairs: HashMap<AccountType, CommsMain>,
}

impl IdentityManager {
    pub fn new(db: Database, config: IdentityManagerConfig) -> Result<Self> {
        let (tx1, recv1) = unbounded();

        Ok(IdentityManager {
            db: db,
            comms: CommsTable {
                to_main: tx1.clone(),
                listener: recv1,
                pairs: HashMap::new(),
            },
            config: config,
        })
    }
    pub fn register_comms(&mut self, account_ty: AccountType) -> CommsVerifier {
        let (cm, cv) = generate_comms(self.comms.to_main.clone(), account_ty.clone());
        self.comms.pairs.insert(account_ty, cm);
        cv
    }
    pub fn get_comms(&self, account_ty: &AccountType) -> StdResult<&CommsMain, ManagerError> {
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
                ExistingDisplayNames { accounts } => {
                    for account in &accounts {
                        // TODO: Create a function for batch insert
                        self.db.insert_display_name(None, account).await?;
                    }
                }
                JudgementGivenAck { net_account } => {
                    self.db.delete_identity(&net_account).await?;
                    self.get_comms(&AccountType::Matrix)?
                        .leave_matrix_room(net_account);
                }
                _ => panic!("Received unrecognized message type. Report as a bug"),
            }
        }

        self.handle_verification_timeouts().await?;

        Ok(())
    }
    async fn handle_verification_timeouts(&self) -> Result<()> {
        let net_accounts = self
            .db
            .select_timed_out_identities(self.config.judgement_timeout_limit)
            .await?;
        let connector_comms = self.get_comms(&AccountType::ReservedConnector)?;

        for net_account in net_accounts {
            info!(
                "Notifying Watcher about timed-out judgement request from: {}",
                net_account.as_str()
            );
            connector_comms.notify_identity_judgment(net_account.clone(), Judgement::Erroneous);

            // TODO: Should be done after Watcher confirmation.
            self.db.delete_identity(&net_account).await?;
            self.get_comms(&AccountType::Matrix)?
                .leave_matrix_room(net_account);
        }

        Ok(())
    }
    async fn handle_new_judgment_request(&mut self, mut ident: OnChainIdentity) -> Result<()> {
        debug!(
            "Handling new judgment request for account: {}",
            ident.net_account().as_str()
        );

        // Check the current, associated addresses of the identity, if any.
        let existing_accounts = self
            .db
            .select_account_statuses(&ident.net_account())
            .await?;

        // Account types to delete **from the identity info** before it gets
        // inserted into the database and therefore prevents replacement.
        let mut to_delete = vec![];

        // Find duplicates.
        let mut contains_non_whitelisted = false;
        for state in ident.account_states() {
            // Reject the entire judgment request if a non-white listed account type is specified.
            if !WHITELIST.contains(&state.account_ty) {
                warn!(
                    "Reject identity {}, use of unacceptable account type: {:?}",
                    ident.net_account().as_str(),
                    state.account_ty
                );

                contains_non_whitelisted = true;

                self.db
                    .set_account_status(
                        ident.net_account(),
                        &state.account_ty,
                        &AccountStatus::Unsupported,
                    )
                    .await?;
            }

            // If the same account already exists in storage and is valid or
            // unconfirmed, remove it (and avoid replacement).
            if existing_accounts
                .iter()
                .find(|&(account_ty, _, status)| {
                    account_ty == &state.account_ty
                        && (status == &AccountStatus::Valid || status == &AccountStatus::Unknown)
                })
                .is_some()
            {
                to_delete.push(state.account_ty.clone());
            }
        }

        // If the identity contains unallowed fields, notify the user about it
        // and prevent accepting the identity.
        if contains_non_whitelisted {
            self.handle_status_change(ident.net_account().clone())
                .await?;

            return Ok(());
        }

        for ty in to_delete {
            ident.remove_account_state(&ty)?;
        }

        // Insert identity into storage, notify tasks for verification.
        self.db.insert_identity(&ident).await?;

        for state in ident.account_states() {
            if state.account_ty == AccountType::Twitter {
                self.db.reset_init_message(&state.account).await?;
            }

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

        if self.db.is_fully_verified(&net_account).await? {
            self.db.persist_display_name(&net_account).await?;

            self.get_comms(&AccountType::ReservedConnector)
                .map(|comms| {
                    info!(
                        "Notifying Watcher about fully verified address: {}",
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

            self.db.remove_identity(&net_account).await?;

            return Ok(());
        }

        // If an account is marked invalid, implying the account could not be
        // reached, then find any other valid accounts which can be contacted
        // and informed about the state of the invalid account. The user should
        // then update the on-chain identity. Additionally, there's a special
        // case of `display_name` which can be deemed invalid if it is too
        // similar to another, existing `display_name` in the identity system.

        /// Find a valid account (or attempt unconfirmed) of the identity which
        /// can be notified about an other account's invalidity. Preference for
        /// Matrix, since it's instant, followed by Email then Twitter.
        fn find_valid<'a>(
            account_statuses: &'a [(AccountType, Account, AccountStatus)],
        ) -> Option<(&'a AccountType, &'a Account)> {
            let filtered = account_statuses
                .iter()
                .filter(|(_, _, status)| {
                    status == &AccountStatus::Valid || status == &AccountStatus::Unknown
                })
                .map(|(account_ty, account, _)| (account_ty, account))
                .collect::<Vec<(&AccountType, &Account)>>();

            for to_notify in &NOTIFY_QUEUE {
                for (account_ty, account) in &filtered {
                    if &to_notify == account_ty {
                        return Some((account_ty, account));
                    }
                }
            }

            None
        }

        /// Find invalid accounts.
        fn find_invalid<'a>(
            account_statuses: &[(AccountType, Account, AccountStatus)],
        ) -> Vec<(AccountType, Account, AccountStatus)> {
            account_statuses
                .iter()
                .cloned()
                .filter(|(_, _, status)| {
                    status == &AccountStatus::Invalid || status == &AccountStatus::Unsupported
                })
                .collect::<Vec<(AccountType, Account, AccountStatus)>>()
        }

        let account_statuses = self.db.select_account_statuses(&net_account).await?;

        // If invalid accounts were found, attempt to contact the user in order
        // to inform that person about the current state of invalid accounts.
        let invalid_accounts = find_invalid(&account_statuses);
        if !invalid_accounts.is_empty() {
            if let Some((to_notify, account)) = find_valid(&account_statuses) {
                self.get_comms(to_notify).map(|comms| {
                    comms.notify_invalid_accounts(
                        net_account.clone(),
                        account.clone(),
                        invalid_accounts.clone(),
                    );
                })?;

                // Mark invalid accounts as notified.
                for (account_ty, _, _) in &invalid_accounts {
                    self.db
                        .set_account_status(&net_account, &account_ty, &AccountStatus::Notified)
                        .await?;
                }
            } else {
                warn!("Identity {} could not be informed about invalid accounts (no valid accounts yet)", net_account.as_str());
            }
        }

        Ok(())
    }
}
