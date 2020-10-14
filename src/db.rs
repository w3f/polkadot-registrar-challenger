use super::Result;
use crate::adapters::{EmailId, TwitterId};
use crate::manager::{AccountStatus, OnChainIdentity};
use crate::primitives::{
    unix_time, Account, AccountType, Challenge, ChallengeStatus, NetAccount, NetworkAddress,
};
use matrix_sdk::identifiers::RoomId;
use rusqlite::{named_params, params, Connection, OptionalExtension};
use std::convert::TryFrom;
use std::result::Result as StdResult;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Fail)]
pub enum DatabaseError {
    #[fail(display = "Failed to open SQLite database: {}", 0)]
    Open(failure::Error),
    #[fail(display = "Database backend error: {}", 0)]
    BackendError(rusqlite::Error),
    #[fail(display = "SQLite database is not in auto-commit mode")]
    NoAutocommit,
    #[fail(display = "Attempt to change something which does not exist")]
    NoChange,
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(val: rusqlite::Error) -> Self {
        DatabaseError::BackendError(val)
    }
}

#[derive(Clone)]
pub struct Database2 {
    con: Arc<Mutex<Connection>>,
}

impl Database2 {
    pub fn new(path: &str) -> Result<Self> {
        let con = Connection::open(path).map_err(|err| DatabaseError::Open(err.into()))?;
        if !con.is_autocommit() {
            return Err(failure::Error::from(DatabaseError::NoAutocommit));
        }

        // Table for pending identities.
        con.execute(
            "CREATE TABLE IF NOT EXISTS pending_judgments (
                id           INTEGER PRIMARY KEY,
                net_account  TEXT NOT NULL UNIQUE,
                created      INTEGER NOT NULL
            )",
            params![],
        )?;

        // Table for account status.
        con.execute(
            "CREATE TABLE IF NOT EXISTS account_status (
                    id      INTEGER PRIMARY KEY,
                    status  TEXT NOT NULL UNIQUE
            )",
            params![],
        )?;

        // TODO: This should be improved -> what if the enum adds new types?
        con.execute(
            "INSERT OR IGNORE INTO account_status
                    (status)
                VALUES
                    ('unknown'),
                    ('valid'),
                    ('invalid'),
                    ('notified')
            ",
            params![],
        )?;

        // Table for challenge status.
        con.execute(
            "CREATE TABLE IF NOT EXISTS challenge_status (
                    id      INTEGER PRIMARY KEY,
                    status  TEXT NOT NULL UNIQUE
            )",
            params![],
        )?;

        // TODO: This should be improved -> what if the enum adds new types?
        con.execute(
            "INSERT OR IGNORE INTO challenge_status
                    (status)
                VALUES
                    ('unconfirmed'),
                    ('accepted'),
                    ('rejected')
            ",
            params![],
        )?;

        // Table for account type.
        con.execute(
            "CREATE TABLE IF NOT EXISTS account_types (
                    id          INTEGER PRIMARY KEY,
                    account_ty  TEXT NOT NULL UNIQUE
            )",
            params![],
        )?;

        // TODO: This should be improved -> what if the enum adds new types?
        con.execute(
            "INSERT OR IGNORE INTO account_types
                    (account_ty)
                VALUES
                    ('legal_name'),
                    ('display_name'),
                    ('email'),
                    ('web'),
                    ('twitter'),
                    ('matrix')
            ",
            params![],
        )?;

        // Table for account state.
        con.execute(
            "CREATE TABLE IF NOT EXISTS account_states (
                id                   INTEGER PRIMARY KEY,
                net_account_id       INTEGER NOT NULL,
                account              TEXT NOT NULL,
                account_ty_id        INTEGER NOT NULL,
                account_status_id    INTEGER NOT NULL,
                challenge            TEXT NOT NULL,
                challenge_status_id  INTEGER NOT NULL,

                UNIQUE (net_account_id, account_ty_id)

                FOREIGN KEY (net_account_id)
                    REFERENCES pending_judgments (id)
                        ON DELETE CASCADE,

                FOREIGN KEY (account_ty_id)
                    REFERENCES account_types (id),

                FOREIGN KEY (account_status_id)
                    REFERENCES account_status (id),

                FOREIGN KEY (challenge_status_id)
                    REFERENCES challenge_status (id)
            )",
            params![],
        )?;

        // Table for known Matrix rooms.
        con.execute(
            "CREATE TABLE IF NOT EXISTS known_matrix_rooms (
                id              INTEGER PRIMARY KEY,
                net_account_id  INTEGER NOT NULL UNIQUE,
                room_id         TEXT,

                FOREIGN KEY (net_account_id)
                    REFERENCES pending_judgments (id)
                        ON DELETE CASCADE
            )",
            params![],
        )?;

        // Table for known Twitter IDs.
        con.execute(
            "
            CREATE TABLE IF NOT EXISTS known_twitter_ids (
                id              INTEGER PRIMARY KEY,
                account         INTEGER NOT NULL UNIQUE,
                twitter_id      TEXT NOT NULL,
                init_msg        INTEGER NOT NULL,
                timestamp       INTEGER NOT NULL
            )
        ",
            params![],
        )?;

        // Table for watermark.
        con.execute(
            "
            CREATE TABLE IF NOT EXISTS watermarks (
                id             INTEGER PRIMARY KEY,
                account_ty_id  INTEGER NOT NULL UNIQUE,
                watermark      INTEGER NOT NULL,

                FOREIGN KEY (account_ty_id)
                    REFERENCES account_types (id)
            )
        ",
            params![],
        )?;

        // Table for processed email IDs.
        con.execute(
            "
            CREATE TABLE IF NOT EXISTS email_processed_ids (
                id          INTEGER PRIMARY KEY,
                email_id    INTEGER NOT NULL UNIQUE,
                timestamp   INTEGER NOT NULL
            )
        ",
            params![],
        )?;

        // Table for all display names.
        con.execute(
            "
            CREATE TABLE IF NOT EXISTS display_names (
                id    INTEGER PRIMARY KEY,
                name  TEXT NOT NULL UNIQUE
            )
        ",
            params![],
        )?;

        Ok(Database2 {
            con: Arc::new(Mutex::new(con)),
        })
    }
    pub async fn insert_identity(&mut self, ident: &OnChainIdentity) -> Result<()> {
        self.insert_identity_batch(&[ident]).await
    }
    pub async fn insert_identity_batch(&mut self, idents: &[&OnChainIdentity]) -> Result<()> {
        let mut con = self.con.lock().await;
        let transaction = con.transaction()?;

        {
            let mut stmt = transaction.prepare(
                "INSERT OR IGNORE INTO pending_judgments (
                    net_account,
                    created
                ) VALUES (
                    :net_account,
                    :timestamp
                )
                ",
            )?;

            for ident in idents {
                stmt.execute_named(named_params! {
                    ":net_account": ident.net_account(),
                    ":timestamp": unix_time() as i64,
                })?;
            }

            let mut stmt = transaction.prepare(
                "
                INSERT OR REPLACE INTO account_states (
                    net_account_id,
                    account,
                    account_ty_id,
                    account_status_id,
                    challenge,
                    challenge_status_id
                ) VALUES (
                    (SELECT id FROM pending_judgments
                        WHERE net_account = :net_account),
                    :account,
                    (SELECT id FROM account_types
                        WHERE account_ty = :account_ty),
                    (SELECT id FROM account_status
                        WHERE status = :account_status),
                    :challenge,
                    (SELECT id FROM challenge_status
                        WHERE status = :challenge_status)
                )",
            )?;

            for ident in idents {
                for state in ident.account_states() {
                    stmt.execute_named(named_params! {
                        ":net_account": ident.net_account(),
                        ":account": &state.account,
                        ":account_ty": &state.account_ty,
                        ":account_status": &state.account_status,
                        ":challenge": &state.challenge.as_str(),
                        ":challenge_status": &state.challenge_status,
                    })?;
                }
            }
        }

        transaction.commit()?;

        Ok(())
    }
    pub async fn remove_identity(&self, net_account: &NetAccount) -> Result<()> {
        let con = self.con.lock().await;

        con.execute_named(
            "
            DELETE FROM
                pending_judgments
            WHERE
                net_account = :net_account
        ",
            named_params! {
                ":net_account": net_account,
            },
        )?;

        Ok(())
    }
    // TODO: Should return AccountType, too.
    pub async fn select_addresses(&self, net_account: &NetAccount) -> Result<Vec<Account>> {
        let con = self.con.lock().await;

        let mut stmt = con.prepare(
            "
            SELECT
                account
            FROM
                account_states
            WHERE
                net_account_id = (
                    SELECT
                        id
                    FROM
                        pending_judgments
                    WHERE
                        net_account = :net_account
                )
        ",
        )?;

        let mut rows = stmt.query_named(named_params! {
            ":net_account": net_account,
        })?;

        let mut net_accounts = vec![];
        while let Some(row) = rows.next()? {
            net_accounts.push(row.get::<_, Account>(0)?);
        }

        Ok(net_accounts)
    }
    #[cfg(test)]
    async fn select_identities(&self) -> Result<Vec<OnChainIdentity>> {
        let con = self.con.lock().await;
        let mut stmt = con.prepare(
            "
            SELECT
                net_account, account_ty, account
            FROM
                pending_judgments
            LEFT JOIN account_states
                ON pending_judgments.id = account_states.net_account_id
            LEFT JOIN account_types
                ON account_states.account_ty_id = account_types.id
        ",
        )?;

        let mut idents: Vec<OnChainIdentity> = vec![];

        let mut rows = stmt.query(params![])?;
        while let Some(row) = rows.next()? {
            let net_account = row.get::<_, NetAccount>(0)?;
            let account_ty = row.get::<_, AccountType>(1)?;

            if let Some(ident) = idents
                .iter_mut()
                .find(|ident| ident.net_account() == &net_account)
            {
                ident.push_account(account_ty, row.get::<_, Account>(2)?)?;
            } else {
                let mut ident = OnChainIdentity::new(net_account)?;
                ident.push_account(account_ty, row.get::<_, Account>(2)?)?;

                idents.push(ident);
            }
        }

        Ok(idents)
    }
    // TODO: Should be account instead of net_account.
    pub async fn insert_room_id(&self, net_account: &NetAccount, room_id: &RoomId) -> Result<()> {
        self.con.lock().await.execute_named(
            "INSERT OR REPLACE INTO known_matrix_rooms (
                    net_account_id,
                    room_id
                ) VALUES (
                    (SELECT id FROM pending_judgments WHERE net_account = :net_account),
                    :room_id
                )",
            named_params! {
                ":net_account": net_account,
                ":room_id": room_id.as_str(),
            },
        )?;

        Ok(())
    }
    pub async fn select_room_id(&self, net_account: &NetAccount) -> Result<Option<RoomId>> {
        let con = self.con.lock().await;
        con.query_row_named(
            "SELECT room_id
                FROM known_matrix_rooms
                WHERE net_account_id =
                    (SELECT id from pending_judgments
                        WHERE
                        net_account = :net_account)
                ",
            named_params! {
                ":net_account": net_account,
            },
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|err| failure::Error::from(err))
        .and_then(|data| {
            if let Some(data) = data {
                Ok(Some(
                    RoomId::try_from(data).map_err(|err| failure::Error::from(err))?,
                ))
            } else {
                Ok(None)
            }
        })
    }
    pub async fn select_room_ids(&self) -> Result<Vec<RoomId>> {
        let con = self.con.lock().await;
        let mut stmt = con.prepare("SELECT room_id FROM known_matrix_rooms")?;

        let mut rows = stmt.query(params![])?;

        let mut room_ids = vec![];
        // `Rows` does not implement `Iterator`.
        while let Some(row) = rows.next()? {
            room_ids.push(RoomId::try_from(row.get::<_, String>(0)?)?);
        }

        Ok(room_ids)
    }
    pub async fn set_account_status(
        &self,
        net_account: &NetAccount,
        account_ty: &AccountType,
        status: &AccountStatus,
    ) -> StdResult<(), DatabaseError> {
        let con = self.con.lock().await;

        con.execute_named(
            "UPDATE
                    account_states
                SET account_status_id =
                    (SELECT id FROM account_status
                        WHERE status = :account_status)
                WHERE
                    net_account_id =
                        (SELECT id FROM pending_judgments
                            WHERE net_account = :net_account)
                AND
                    account_ty_id =
                        (SELECT id FROM account_types
                            WHERE account_ty = :account_ty)
            ",
            named_params! {
                ":account_status": status,
                ":net_account": net_account,
                ":account_ty": account_ty,
            },
        )
        .map_err(|err| err.into())
        .and_then(|changes| {
            if changes == 0 {
                Err(DatabaseError::NoChange)
            } else {
                Ok(changes)
            }
        })?;

        Ok(())
    }
    pub async fn set_challenge_status(
        &self,
        net_account: &NetAccount,
        account_ty: &AccountType,
        status: &ChallengeStatus,
    ) -> Result<()> {
        self.con.lock().await.execute_named(
            "UPDATE
                    account_states
                SET challenge_status_id =
                    (SELECT id FROM challenge_status
                        WHERE status = :challenge_status)
                WHERE
                    net_account_id =
                        (SELECT id FROM pending_judgments
                            WHERE net_account = :net_account)
                AND
                    account_ty_id =
                        (SELECT id FROM account_types
                            WHERE account_ty = :account_ty)
            ",
            named_params! {
                ":challenge_status": status,
                ":net_account": net_account,
                ":account_ty": account_ty,
            },
        )?;

        Ok(())
    }
    pub async fn select_challenge_data(
        &self,
        account: &Account,
        account_ty: &AccountType,
    ) -> Result<Vec<(NetworkAddress, Challenge)>> {
        let con = self.con.lock().await;

        // TODO: Figure out why `IN` does not work here...
        let mut stmt = con.prepare(
            "
            SELECT
                net_account, challenge
            FROM
                pending_judgments
            INNER JOIN
                account_states
            ON
                pending_judgments.id = account_states.net_account_id
            WHERE
                account_states.account = :account
            AND
                account_states.challenge_status_id != (
                    SELECT
                        id
                    FROM
                        challenge_status
                    WHERE
                        status = 'accepted'
                )
            AND
                account_states.account_ty_id = (
                    SELECT
                        id
                    FROM
                        account_types
                    WHERE
                        account_ty = :account_ty
                )
        ",
        )?;

        let mut rows = stmt.query_named(named_params! {
            ":account": account,
            ":account_ty": account_ty
        })?;

        let mut challenge_set = vec![];
        while let Some(row) = rows.next()? {
            challenge_set.push((
                NetworkAddress::try_from(row.get::<_, NetAccount>(0)?)?,
                Challenge(row.get::<_, String>(1)?),
            ));
        }

        Ok(challenge_set)
    }
    // Check whether the identity is fully verified.
    pub async fn is_fully_verified(&self, net_account: &NetAccount) -> Result<bool> {
        let con = self.con.lock().await;

        let mut stmt = con.prepare(
            "SELECT
                        status
                    FROM
                        challenge_status
                    LEFT JOIN
                        account_states
                    ON
                        account_states.challenge_status_id = challenge_status.id
                    WHERE
                        account_states.net_account_id = (
                            SELECT
                                id
                            FROM
                                pending_judgments
                            WHERE
                                net_account = :net_account
                        )
                    AND account_ty_id
                        IN (
                            SELECT
                                id
                            FROM
                                account_types
                            WHERE
                                account_ty IN (
                                    'display_name',
                                    'matrix',
                                    'email',
                                    'twitter'
                                )
                        )
                    ",
        )?;

        let mut rows = stmt.query_named(named_params! {
            ":net_account": net_account,
        })?;

        while let Some(row) = rows.next()? {
            if row.get::<_, ChallengeStatus>(0)? != ChallengeStatus::Accepted {
                return Ok(false);
            }
        }

        Ok(true)
    }
    pub async fn select_timed_out_identities(&self, timeout_limit: u64) -> Result<Vec<NetAccount>> {
        let con = self.con.lock().await;

        let mut stmt = con.prepare(
            "
            SELECT
                net_account
            FROM
                pending_judgments
            LEFT JOIN
                account_states
            ON
                pending_judgments.id = account_states.net_account_id
            WHERE
                pending_judgments.created < :timeout_limit
            AND
                account_states.challenge_status_id != (
                    SELECT
                        id
                    FROM
                        challenge_status
                    WHERE
                        status = 'accepted'
                )
        ",
        )?;

        let mut rows = stmt.query_named(named_params! {
            ":timeout_limit": (unix_time() - timeout_limit) as i64,
        })?;

        let mut net_accounts = vec![];
        while let Some(row) = rows.next()? {
            net_accounts.push(row.get::<_, NetAccount>(0)?);
        }

        Ok(net_accounts)
    }
    pub async fn cleanup_timed_out_identities(&self, timeout_limit: u64) -> Result<()> {
        let net_accounts = self.select_timed_out_identities(timeout_limit).await?;
        let con = self.con.lock().await;

        let mut stmt = con.prepare(
            "
            DELETE FROM
                pending_judgments
            WHERE
                net_account = :net_account
        ",
        )?;

        for net_account in net_accounts {
            stmt.execute_named(named_params! {
                ":net_account": net_account,
            })?;
        }

        Ok(())
    }
    pub async fn select_account_statuses(
        &self,
        net_account: &NetAccount,
    ) -> Result<Vec<(AccountType, Account, AccountStatus)>> {
        let mut con = self.con.lock().await;
        let transaction = con.transaction()?;

        let account_set = {
            let mut stmt = transaction.prepare(
                "
            SELECT
                account_ty, account, status
            FROM
                account_states
            INNER JOIN
                account_status
            ON
                account_states.account_status_id =
                    account_status.id
            WHERE
                account_states.net_account_id = (
                    SELECT
                        id
                    FROM
                        pending_judgments
                    WHERE
                        net_account = :net_account
                )
            INNER JOIN
                account_status
            ON
                account_states.account_status_id =
                    account_status.id
            ",
            )?;

            let mut rows = stmt.query_named(named_params! {
                ":net_account": net_account,
            })?;

            let mut account_set = vec![];
            while let Some(row) = rows.next()? {
                account_set.push((
                    row.get::<_, AccountType>(0)?,
                    row.get::<_, Account>(1)?,
                    row.get::<_, AccountStatus>(2)?,
                ))
            }

            account_set
        };

        transaction.commit()?;

        Ok(account_set)
    }
    #[cfg(test)]
    pub async fn insert_twitter_id(&self, account: &Account, twitter_id: &TwitterId) -> Result<()> {
        self.insert_twitter_ids(&[(account, twitter_id)]).await
    }
    pub async fn insert_twitter_ids(&self, pair: &[(&Account, &TwitterId)]) -> Result<()> {
        let con = self.con.lock().await;
        let mut stmt = con.prepare(
            "
            INSERT OR REPLACE INTO
                known_twitter_ids (
                    account,
                    twitter_id,
                    init_msg,
                    timestamp
                )
            VALUES (
                :account,
                :twitter_id,
                '0',
                :timestamp
            )
        ",
        )?;

        for (account, twitter_id) in pair {
            stmt.execute_named(named_params! {
                    ":account": account,
                    ":twitter_id": twitter_id,
                    ":timestamp": unix_time() as i64,
            })?;
        }

        Ok(())
    }
    pub async fn select_account_from_twitter_id(
        &self,
        twitter_id: &TwitterId,
    ) -> Result<Option<(Account, bool)>> {
        let con = self.con.lock().await;
        con.query_row_named(
            "
            SELECT
                account, init_msg
            FROM
                known_twitter_ids
            WHERE
                twitter_id = :twitter_id

        ",
            named_params! {
                ":twitter_id": twitter_id,
            },
            |row| Ok((row.get::<_, Account>(0)?, row.get::<_, bool>(1)?)),
        )
        .optional()
        .map_err(|err| failure::Error::from(err))
    }
    pub async fn confirm_init_message(&self, account: &Account) -> Result<()> {
        let con = self.con.lock().await;
        con.execute_named(
            "
            UPDATE
                known_twitter_ids
            SET
                init_msg = 1
            WHERE
                account = :account
        ",
            named_params! {
                ":account": account,
            },
        )?;

        Ok(())
    }
    pub async fn select_watermark(&self, account_ty: &AccountType) -> Result<Option<u64>> {
        let con = self.con.lock().await;
        con.query_row_named(
            "SELECT
                watermark
            FROM
                watermarks
            WHERE
                account_ty_id = (
                    SELECT
                        id
                    FROM
                        account_types
                    WHERE
                        account_ty = :account_ty
                )
            ",
            named_params! {
                ":account_ty": account_ty,
            },
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|err| failure::Error::from(err))
        .map(|v| v.map(|v| v as u64))
    }
    pub async fn update_watermark(&self, account_ty: &AccountType, value: u64) -> Result<()> {
        let con = self.con.lock().await;
        con.execute_named(
            "
            INSERT OR REPlACE INTO watermarks (
                account_ty_id,
                watermark
            ) VALUES (
                (
                    SELECT
                        id
                    FROM
                        account_types
                    WHERE
                        account_ty = :account_ty
                ),
                :value
            )
        ",
            named_params! {
                ":value": value as i64,
                ":account_ty": account_ty,
            },
        )?;

        Ok(())
    }
    pub async fn track_email_id(&self, email_id: &EmailId) -> Result<()> {
        let con = self.con.lock().await;

        con.execute_named(
            "
            INSERT OR IGNORE INTO email_processed_ids (
                email_id,
                timestamp
            ) VALUES (
                :email_id,
                :timestamp
            )
            ",
            named_params! {
                ":email_id": email_id,
                ":timestamp": unix_time() as i64,
            },
        )?;

        Ok(())
    }
    pub async fn find_untracked_email_ids<'id>(
        &self,
        ids: &'id [EmailId],
    ) -> Result<Vec<&'id EmailId>> {
        let con = self.con.lock().await;
        let mut stmt = con.prepare(
            "
            SELECT
                id
            FROM
                email_processed_ids
            WHERE
                email_id = :email_id
        ",
        )?;

        let mut untracked_email_ids = vec![];
        for email_id in ids {
            stmt.query_row_named(
                named_params! {
                    ":email_id": email_id
                },
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(|_| ())
            .or_else(|| {
                untracked_email_ids.push(email_id);
                Some(())
            });
        }

        Ok(untracked_email_ids)
    }
    pub async fn insert_display_name(&self, account: &Account) -> Result<()> {
        let con = self.con.lock().await;

        con.execute_named(
            "
            INSERT OR IGNORE INTO display_names (
                name
            ) VALUES (
                :account
            )
        ",
            named_params! {
                ":account": account,
            },
        )?;

        Ok(())
    }
    pub async fn select_display_names(&self) -> Result<Vec<Account>> {
        let con = self.con.lock().await;

        let mut stmt = con.prepare(
            "
            SELECT
                name
            FROM
                display_names
        ",
        )?;

        let mut rows = stmt.query(params![])?;

        let mut accounts = vec![];
        while let Some(row) = rows.next()? {
            accounts.push(row.get::<_, Account>(0)?);
        }

        Ok(accounts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{EmailId, TwitterId};
    use crate::primitives::{Challenge, NetAccount};
    use tokio::runtime::Runtime;
    use tokio::time::{self, Duration};

    // Generate a random db path
    fn db_path() -> String {
        format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
    }

    #[test]
    fn database_setup() {
        let path = db_path();

        // Test repeated initialization.
        let _db = Database2::new(&path).unwrap();
        let _db = Database2::new(&path).unwrap();
    }

    #[test]
    fn insert_identity() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            let mut ident = OnChainIdentity::new(NetAccount::from(
                "14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU",
            ))
            .unwrap();

            ident
                .push_account(AccountType::DisplayName, Account::from("Alice"))
                .unwrap();

            ident
                .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                .unwrap();

            // Insert and check return value.
            let _ = db.insert_identity(&ident).await.unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(
                res[0]
                    .get_account_state(&AccountType::Matrix)
                    .as_ref()
                    .unwrap()
                    .account,
                Account::from("@alice:matrix.org")
            );

            // Repeated insert of same value.
            let _ = db.insert_identity(&ident).await.unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(
                res[0]
                    .get_account_state(&AccountType::Matrix)
                    .as_ref()
                    .unwrap()
                    .account,
                Account::from("@alice:matrix.org")
            );

            // Change a field, insert and return value.
            // (Reset account state)
            let mut ident = OnChainIdentity::new(NetAccount::from(
                "14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU",
            ))
            .unwrap();

            ident
                .push_account(AccountType::DisplayName, Account::from("Alice"))
                .unwrap();

            ident
                .push_account(
                    AccountType::Matrix,
                    Account::from("@alice_second:matrix.org"),
                )
                .unwrap();

            let _ = db.insert_identity(&ident).await.unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(
                res[0]
                    .get_account_state(&AccountType::Matrix)
                    .as_ref()
                    .unwrap()
                    .account,
                Account::from("@alice_second:matrix.org")
            );

            // Additional identity
            let mut ident = OnChainIdentity::new(NetAccount::from(
                "163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C",
            ))
            .unwrap();

            ident
                .push_account(AccountType::DisplayName, Account::from("Bob"))
                .unwrap();

            ident
                .push_account(AccountType::Matrix, Account::from("@bob:matrix.org"))
                .unwrap();

            let _ = db.insert_identity(&ident).await.unwrap();

            // Select identities
            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 2);
            assert_eq!(
                res[0]
                    .get_account_state(&AccountType::Matrix)
                    .unwrap()
                    .account,
                Account::from("@alice_second:matrix.org")
            );

            assert_eq!(
                res[1]
                    .get_account_state(&AccountType::Matrix)
                    .unwrap()
                    .account,
                Account::from("@bob:matrix.org")
            );

            // Delete identity
            db.remove_identity(&NetAccount::from(
                "163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C",
            ))
            .await
            .unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(
                res[0]
                    .get_account_state(&AccountType::Matrix)
                    .unwrap()
                    .account,
                Account::from("@alice_second:matrix.org")
            );
        });
    }

    #[test]
    fn select_addresses() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            // Create identity.
            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");
            let mut alice_ident = OnChainIdentity::new(alice.clone()).unwrap();

            let res = db.select_addresses(&alice).await.unwrap();
            assert_eq!(res.len(), 0);

            alice_ident
                .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                .unwrap();

            alice_ident
                .push_account(AccountType::Email, Account::from("alice@example.com"))
                .unwrap();

            alice_ident
                .push_account(AccountType::Twitter, Account::from("@alice"))
                .unwrap();

            db.insert_identity(&alice_ident).await.unwrap();

            let res = db.select_addresses(&alice).await.unwrap();
            assert_eq!(res.len(), 3);
            assert!(res.contains(&Account::from("@alice:matrix.org")));
            assert!(res.contains(&Account::from("alice@example.com")));
            assert!(res.contains(&Account::from("@alice")));
        });
    }

    #[test]
    fn select_cleanup_timed_out_identities() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            // Prepare addresses.
            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");
            let bob = NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C");

            // Create identities
            let mut alice_ident = OnChainIdentity::new(alice.clone()).unwrap();

            alice_ident
                .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                .unwrap();

            let mut bob_ident = OnChainIdentity::new(bob.clone()).unwrap();

            bob_ident
                .push_account(AccountType::Matrix, Account::from("@bob:matrix.org"))
                .unwrap();

            db.insert_identity_batch(&[&alice_ident, &bob_ident])
                .await
                .unwrap();

            let res = db.select_timed_out_identities(3).await.unwrap();
            assert!(res.is_empty());

            time::delay_for(Duration::from_secs(4)).await;

            // Add new identity
            let eve = NetAccount::from("13gjXZKFPCELoVN56R2KopsNKAb6xqHwaCfWA8m4DG4s9xGQ");
            let mut eve_ident = OnChainIdentity::new(eve.clone()).unwrap();
            eve_ident
                .push_account(AccountType::Matrix, Account::from("@eve:matrix.org"))
                .unwrap();

            db.insert_identity(&eve_ident).await.unwrap();

            let res = db.select_timed_out_identities(3).await.unwrap();
            assert_eq!(res.len(), 2);
            assert!(res.contains(&alice));
            assert!(res.contains(&bob));

            let res = db.select_identities().await.unwrap();
            let accounts: Vec<&NetAccount> = res.iter().map(|ident| ident.net_account()).collect();
            assert_eq!(accounts.len(), 3);
            assert!(accounts.contains(&&alice));
            assert!(accounts.contains(&&bob));
            assert!(accounts.contains(&&eve));

            db.cleanup_timed_out_identities(3).await.unwrap();

            let res = db.select_identities().await.unwrap();
            let accounts: Vec<&NetAccount> = res.iter().map(|ident| ident.net_account()).collect();
            assert_eq!(accounts.len(), 1);
            assert!(accounts.contains(&&eve));
        });
    }

    #[test]
    fn insert_identity_batch() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            let idents = vec![
                // Two identical identities with the same values.
                {
                    let mut ident = OnChainIdentity::new(NetAccount::from(
                        "14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU",
                    ))
                    .unwrap();
                    ident
                        .push_account(AccountType::DisplayName, Account::from("Alice"))
                        .unwrap();
                    ident
                        .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                        .unwrap();
                    ident
                },
                {
                    let mut ident = OnChainIdentity::new(NetAccount::from(
                        "14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU",
                    ))
                    .unwrap();
                    ident
                        .push_account(AccountType::DisplayName, Account::from("Alice"))
                        .unwrap();
                    ident
                        .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                        .unwrap();
                    ident
                },
                // Two identical identities with varying values (matrix).
                {
                    let mut ident = OnChainIdentity::new(NetAccount::from(
                        "163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C",
                    ))
                    .unwrap();
                    ident
                        .push_account(AccountType::DisplayName, Account::from("Bob"))
                        .unwrap();
                    ident
                        .push_account(AccountType::Matrix, Account::from("@bob:matrix.org"))
                        .unwrap();
                    ident
                },
                {
                    let mut ident = OnChainIdentity::new(NetAccount::from(
                        "163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C",
                    ))
                    .unwrap();
                    ident
                        .push_account(AccountType::DisplayName, Account::from("Bob"))
                        .unwrap();
                    ident
                        .push_account(AccountType::Matrix, Account::from("@bob_second:matrix.org"))
                        .unwrap();
                    ident
                },
            ];

            let idents: Vec<&OnChainIdentity> = idents.iter().map(|ident| ident).collect();

            let _ = db.insert_identity_batch(&idents).await.unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 2);
            res.iter()
                .find(|ident| {
                    ident.net_account()
                        == &NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU")
                })
                .map(|ident| {
                    assert_eq!(
                        ident
                            .get_account_state(&AccountType::Matrix)
                            .unwrap()
                            .account,
                        Account::from("@alice:matrix.org")
                    );
                    Some(ident)
                })
                .unwrap();

            res.iter()
                .find(|ident| {
                    ident.net_account()
                        == &NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C")
                })
                .map(|ident| {
                    assert_eq!(
                        ident
                            .get_account_state(&AccountType::Matrix)
                            .unwrap()
                            .account,
                        Account::from("@bob_second:matrix.org")
                    );
                    Some(ident)
                })
                .unwrap();
        });
    }

    #[test]
    fn insert_select_room_id() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            // Prepare addresses.
            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");
            let bob = NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C");
            let eve = NetAccount::from("13gjXZKFPCELoVN56R2KopsNKAb6xqHwaCfWA8m4DG4s9xGQ");

            // Create identity
            let ident = OnChainIdentity::new(alice.clone()).unwrap();

            // Insert and check return value.
            let _ = db.insert_identity(&ident).await.unwrap();

            // Create identity
            let ident = OnChainIdentity::new(NetAccount::from(
                "163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C",
            ))
            .unwrap();

            // Insert and check return value.
            let _ = db.insert_identity(&ident).await.unwrap();

            // Test RoomId functionality.
            let alice_room_1 = RoomId::try_from("!ALICE1:matrix.org").unwrap();
            let alice_room_2 = RoomId::try_from("!ALICE2:matrix.org").unwrap();
            let bob_room = RoomId::try_from("!BOB:matrix.org").unwrap();

            // Insert RoomIds
            db.insert_room_id(&alice, &alice_room_1).await.unwrap();
            db.insert_room_id(&bob, &bob_room).await.unwrap();

            // Updated data with repeated inserts.
            db.insert_room_id(&alice, &alice_room_2).await.unwrap();
            db.insert_room_id(&alice, &alice_room_2).await.unwrap();

            let res = db.select_room_id(&alice).await.unwrap().unwrap();
            assert_eq!(res, alice_room_2);

            let res = db.select_room_id(&bob).await.unwrap().unwrap();
            assert_eq!(res, bob_room);

            // Does not exists.
            let res = db.select_room_id(&eve).await.unwrap();
            assert!(res.is_none());

            let res = db.select_room_ids().await.unwrap();
            assert_eq!(res.len(), 2);
            assert!(res.contains(&alice_room_2));
            assert!(res.contains(&bob_room));
        });
    }

    #[test]
    fn set_challenge_status() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");

            // Create and insert identity into storage.
            let mut ident = OnChainIdentity::new(alice.clone()).unwrap();
            ident
                .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                .unwrap();
            ident
                .push_account(AccountType::Email, Account::from("alice@example.com"))
                .unwrap();

            db.insert_identity(&ident).await.unwrap();

            // Alice is not verified.
            let res = db.is_fully_verified(&alice).await.unwrap();
            assert_eq!(res, false);

            // Accept an additional account.
            db.set_challenge_status(&alice, &AccountType::Matrix, &ChallengeStatus::Accepted)
                .await
                .unwrap();

            // Not all essential accounts have been verified yet.
            let res = db.is_fully_verified(&alice).await.unwrap();
            assert_eq!(res, false);

            // Accept an account.
            db.set_challenge_status(&alice, &AccountType::Email, &ChallengeStatus::Accepted)
                .await
                .unwrap();

            // All essential accounts have been verified.
            let res = db.is_fully_verified(&alice).await.unwrap();
            assert_eq!(res, true);
        });
    }

    #[test]
    fn select_challenge_data() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            // Prepare addresses.
            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");
            let bob = NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C");

            // Create identity
            let mut ident = OnChainIdentity::new(alice.clone()).unwrap();
            ident
                .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                .unwrap();
            ident
                .push_account(AccountType::Web, Account::from("alice.com"))
                .unwrap();

            // Insert and check return value.
            let _ = db.insert_identity(&ident).await.unwrap();

            // Create identity with the same Matrix account.
            let mut ident = OnChainIdentity::new(bob.clone()).unwrap();
            ident
                .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                .unwrap();
            ident
                .push_account(AccountType::Web, Account::from("bob.com"))
                .unwrap();

            // Insert and check return value.
            let _ = db.insert_identity(&ident).await.unwrap();

            let res = db
                .select_challenge_data(&Account::from("@alice:matrix.org"), &AccountType::Matrix)
                .await
                .unwrap();
            assert_eq!(res.len(), 2);
            assert_eq!(res[0].0, NetworkAddress::try_from(alice).unwrap());
            assert_eq!(res[1].0, NetworkAddress::try_from(bob).unwrap());
        });
    }

    #[test]
    fn set_account_status() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");

            // Alice does not exists.
            let res = db
                .set_account_status(&alice, &AccountType::Matrix, AccountStatus::Valid)
                .await;
            assert!(res.is_err());

            // Create and insert identity into storage.
            let mut ident = OnChainIdentity::new(alice.clone()).unwrap();
            ident
                .push_account(AccountType::Matrix, Account::from("@alice:matrix.org"))
                .unwrap();
            ident
                .push_account(AccountType::Web, Account::from("alice.com"))
                .unwrap();

            db.insert_identity(&ident).await.unwrap();

            // Set account status to valid
            db.set_account_status(&alice, &AccountType::Matrix, AccountStatus::Valid)
                .await
                .unwrap();
        });
    }

    #[test]
    fn select_confirm_watermark() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let db = Database2::new(&db_path()).unwrap();

            let res = db.select_watermark(&AccountType::Twitter).await.unwrap();
            assert!(res.is_none());

            db.update_watermark(&AccountType::Email, 50).await.unwrap();
            db.update_watermark(&AccountType::Twitter, 100)
                .await
                .unwrap();

            let res = db
                .select_watermark(&AccountType::Email)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(res, 50);

            let res = db
                .select_watermark(&AccountType::Twitter)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(res, 100);
        });
    }

    #[test]
    fn select_insert_twitter_id() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let db = Database2::new(&db_path()).unwrap();

            let alice = Account::from("Alice");
            let alice_id = TwitterId::from(1000);

            let bob = Account::from("Bob");
            let bob_id = TwitterId::from(2000);

            let res = db.select_account_from_twitter_id(&alice_id).await.unwrap();
            assert!(res.is_none());

            let res = db.select_account_from_twitter_id(&bob_id).await.unwrap();
            assert!(res.is_none());

            db.insert_twitter_id(&alice, &alice_id).await.unwrap();
            db.insert_twitter_id(&bob, &bob_id).await.unwrap();

            let (account, init_msg) = db
                .select_account_from_twitter_id(&alice_id)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(account, alice);
            assert_eq!(init_msg, false);

            let (account, init_msg) = db
                .select_account_from_twitter_id(&bob_id)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(account, bob);
            assert_eq!(init_msg, false);

            db.confirm_init_message(&alice).await.unwrap();

            let (account, init_msg) = db
                .select_account_from_twitter_id(&alice_id)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(account, alice);
            assert_eq!(init_msg, true);

            let (account, init_msg) = db
                .select_account_from_twitter_id(&bob_id)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(account, bob);
            assert_eq!(init_msg, false);
        });
    }

    #[test]
    fn email_id_tracking() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let db = Database2::new(&db_path()).unwrap();

            let id_1 = EmailId::from(11u32);
            let id_2 = EmailId::from(22u32);
            let id_3 = EmailId::from(33u32);

            let list = [id_1.clone(), id_2.clone(), id_3.clone()];

            let res = db.find_untracked_email_ids(&list).await.unwrap();
            assert_eq!(&res, &[&id_1, &id_2, &id_3]);

            db.track_email_id(&id_2).await.unwrap();

            let res = db.find_untracked_email_ids(&list).await.unwrap();
            assert_eq!(&res, &[&id_1, &id_3]);

            db.track_email_id(&id_1).await.unwrap();

            let res = db.find_untracked_email_ids(&list).await.unwrap();
            assert_eq!(&res, &[&id_3]);
        });
    }
}
