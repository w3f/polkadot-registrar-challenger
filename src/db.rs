use super::Result;
use crate::adapters::TwitterId;
use crate::identity::{AccountStatus, OnChainIdentity};
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, NetAccount, NetworkAddress,
};
use failure::err_msg;
use matrix_sdk::identifiers::RoomId;
use rocksdb::{ColumnFamily, IteratorMode, Options, DB};
use rusqlite::{named_params, params, Connection, OptionalExtension};
use std::convert::{AsRef, TryFrom};
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
    #[fail(display = "Failed to convert column field into native type")]
    InvalidType,
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

const PENDING_JUDGMENTS: &'static str = "pending_judgments";
const KNOWN_MATRIX_ROOMS: &'static str = "known_matrix_rooms";
const CHALLENGE_STATUS: &'static str = "challenge_status";
const ACCOUNT_STATUS: &'static str = "account_status";
const ACCOUNT_TYPES: &'static str = "account_types";
const ACCOUNT_STATE: &'static str = "account_states";

impl Database2 {
    pub fn new(path: &str) -> Result<Self> {
        let con = Connection::open(path).map_err(|err| DatabaseError::Open(err.into()))?;
        if !con.is_autocommit() {
            return Err(failure::Error::from(DatabaseError::NoAutocommit));
        }

        // Table for pending identities.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                id           INTEGER PRIMARY KEY,
                net_account  TEXT NOT NULL UNIQUE
            )",
                table = PENDING_JUDGMENTS,
            ),
            params![],
        )?;

        // Table for account status.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {tbl_account_status} (
                    id      INTEGER PRIMARY KEY,
                    status  TEXT NOT NULL UNIQUE
            )",
                tbl_account_status = ACCOUNT_STATUS
            ),
            params![],
        )?;

        // TODO: This should be improved -> what if the enum adds new types?
        con.execute(
            &format!(
                "INSERT OR IGNORE INTO {tbl_account_status}
                    (status)
                VALUES
                    ('unknown'),
                    ('valid'),
                    ('invalid'),
                    ('notified')
            ",
                tbl_account_status = ACCOUNT_STATUS,
            ),
            params![],
        )?;

        // Table for challenge status.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {tbl_challenge_status} (
                    id      INTEGER PRIMARY KEY,
                    status  TEXT NOT NULL UNIQUE
            )",
                tbl_challenge_status = CHALLENGE_STATUS
            ),
            params![],
        )?;

        // TODO: This should be improved -> what if the enum adds new types?
        con.execute(
            &format!(
                "INSERT OR IGNORE INTO {tbl_challenge_status}
                    (status)
                VALUES
                    ('unconfirmed'),
                    ('accepted'),
                    ('rejected')
            ",
                tbl_challenge_status = CHALLENGE_STATUS,
            ),
            params![],
        )?;

        // Table for account type.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {tbl_account_ty} (
                    id          INTEGER PRIMARY KEY,
                    account_ty  TEXT NOT NULL UNIQUE
            )",
                tbl_account_ty = ACCOUNT_TYPES
            ),
            params![],
        )?;

        // TODO: This should be improved -> what if the enum adds new types?
        con.execute(
            &format!(
                "INSERT OR IGNORE INTO {tbl_account_ty}
                    (account_ty)
                VALUES
                    ('legal_name'),
                    ('display_name'),
                    ('email'),
                    ('web'),
                    ('twitter'),
                    ('matrix')
            ",
                tbl_account_ty = ACCOUNT_TYPES,
            ),
            params![],
        )?;

        // Table for account state.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table_main} (
                id                   INTEGER PRIMARY KEY,
                net_account_id       INTEGER NOT NULL,
                account              TEXT NOT NULL,
                account_ty_id        INTEGER NOT NULL,
                account_status_id    INTEGER NOT NULL,
                challenge            TEXT NOT NULL,
                challenge_status_id  INTEGER NOT NULL,

                UNIQUE (net_account_id, account_ty_id)

                FOREIGN KEY (net_account_id)
                    REFERENCES {table_identities} (id),

                FOREIGN KEY (account_ty_id)
                    REFERENCES {table_account_ty} (id),

                FOREIGN KEY (account_status_id)
                    REFERENCES {table_account_status} (id),

                FOREIGN KEY (challenge_status_id)
                    REFERENCES {table_challenge_status} (id)
            )",
                table_main = ACCOUNT_STATE,
                table_identities = PENDING_JUDGMENTS,
                table_account_ty = ACCOUNT_TYPES,
                table_account_status = ACCOUNT_STATUS,
                table_challenge_status = CHALLENGE_STATUS,
            ),
            params![],
        )?;

        // Table for known Matrix rooms.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {tbl_matrix_rooms} (
                id              INTEGER PRIMARY KEY,
                net_account_id  INTEGER NOT NULL UNIQUE,
                room_id         TEXT,

                FOREIGN KEY (net_account_id)
                    REFERENCES {tbl_identities} (id)
            )",
                tbl_matrix_rooms = KNOWN_MATRIX_ROOMS,
                tbl_identities = PENDING_JUDGMENTS,
            ),
            params![],
        )?;

        // Table for known Twitter IDs.
        con.execute(
            "
            CREATE TABLE IF NOT EXISTS known_twitter_ids (
                id          INTEGER PRIMARY KEY,
                account     INTEGER NOT NULL UNIQUE,
                twitter_id  TEXT,
                init_msg    INTEGER NOT NULL
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

                FOREIGN KEY (accout_ty_id)
                    REFERENCES account_types (id)
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
            let mut stmt = transaction.prepare(&format!(
                "INSERT OR IGNORE INTO {tbl_identities} (net_account)
                VALUES (:net_account)
                ",
                tbl_identities = PENDING_JUDGMENTS,
            ))?;

            for ident in idents {
                stmt.execute_named(named_params! {
                    ":net_account": ident.net_account()
                })?;
            }

            let mut stmt = transaction.prepare(&format!(
                "
                INSERT OR REPLACE INTO {tbl_account_state} (
                    net_account_id,
                    account,
                    account_ty_id,
                    account_status_id,
                    challenge,
                    challenge_status_id
                ) VALUES (
                    (SELECT id FROM {tbl_identities}
                        WHERE net_account = :net_account),
                    :account,
                    (SELECT id FROM {tbl_account_ty}
                        WHERE account_ty = :account_ty),
                    (SELECT id FROM {tbl_account_status}
                        WHERE status = :account_status),
                    :challenge,
                    (SELECT id FROM {tbl_challenge_status}
                        WHERE status = :challenge_status)
                )",
                tbl_account_state = ACCOUNT_STATE,
                tbl_identities = PENDING_JUDGMENTS,
                tbl_account_ty = ACCOUNT_TYPES,
                tbl_account_status = ACCOUNT_STATUS,
                tbl_challenge_status = CHALLENGE_STATUS,
            ))?;

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
    #[cfg(test)]
    async fn select_identities(&self) -> Result<Vec<OnChainIdentity>> {
        let con = self.con.lock().await;
        let mut stmt = con.prepare(
            "
            SELECT net_account, account_ty, account
            FROM pending_judgments
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
            &format!(
                "INSERT OR REPLACE INTO {tbl_room_id} (
                    net_account_id,
                    room_id
                ) VALUES (
                    (SELECT id FROM {tbl_identities} WHERE net_account = :net_account),
                    :room_id
                )",
                tbl_room_id = KNOWN_MATRIX_ROOMS,
                tbl_identities = PENDING_JUDGMENTS,
            ),
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
            &format!(
                "SELECT room_id
                FROM {tbl_room_id}
                WHERE net_account_id =
                    (SELECT id from {tbl_identities}
                        WHERE
                        net_account = :net_account)
                ",
                tbl_room_id = KNOWN_MATRIX_ROOMS,
                tbl_identities = PENDING_JUDGMENTS,
            ),
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
        let mut stmt = con.prepare(&format!(
            "SELECT room_id FROM {table}",
            table = KNOWN_MATRIX_ROOMS
        ))?;

        let mut rows = stmt.query(params![])?;

        let mut room_ids = vec![];
        // `Rows` does not implement `Iterator`.
        while let Some(row) = rows.next()? {
            room_ids.push(RoomId::try_from(row.get::<_, String>(0)?)?);
        }

        Ok(room_ids)
    }
    pub async fn select_net_account_from_room_id(
        &self,
        room_id: &RoomId,
    ) -> Result<Option<NetAccount>> {
        let con = self.con.lock().await;

        con.query_row_named(
            "SELECT net_account
                FROM pending_judgments
                INNER JOIN known_matrix_rooms
                    ON known_matrix_rooms.net_account_id = pending_judgments.id
                WHERE
                    known_matrix_rooms.room_id = :room_id
                ",
            named_params! {
                ":room_id": room_id.as_str(),
            },
            |row| row.get::<_, NetAccount>(0),
        )
        .optional()
        .map_err(|err| failure::Error::from(err))
    }
    pub async fn set_account_status(
        &self,
        net_account: &NetAccount,
        account_ty: &AccountType,
        status: AccountStatus,
    ) -> StdResult<(), DatabaseError> {
        let con = self.con.lock().await;

        con.execute_named(
            &format!(
                "UPDATE {tbl_update}
                SET account_status_id =
                    (SELECT id FROM {tbl_account_status}
                        WHERE status = :account_status)
                WHERE
                    net_account_id =
                        (SELECT id FROM {tbl_identities}
                            WHERE net_account = :net_account)
                AND
                    account_ty_id =
                        (SELECT id FROM {tbl_acc_types}
                            WHERE account_ty = :account_ty)
            ",
                tbl_update = ACCOUNT_STATE,
                tbl_account_status = ACCOUNT_STATUS,
                tbl_identities = PENDING_JUDGMENTS,
                tbl_acc_types = ACCOUNT_TYPES,
            ),
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
        status: ChallengeStatus,
    ) -> Result<()> {
        self.con.lock().await.execute_named(
            &format!(
                "UPDATE {tbl_update}
                SET challenge_status_id =
                    (SELECT id FROM {tbl_challenge_status}
                        WHERE status = :challenge_status)
                WHERE
                    net_account_id =
                        (SELECT id FROM {tbl_identities}
                            WHERE net_account = :net_account)
                AND
                    account_ty_id =
                        (SELECT id FROM {tbl_acc_types}
                            WHERE account_ty = :account_ty)
            ",
                tbl_update = ACCOUNT_STATE,
                tbl_challenge_status = CHALLENGE_STATUS,
                tbl_identities = PENDING_JUDGMENTS,
                tbl_acc_types = ACCOUNT_TYPES,
            ),
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
    // Check whether the identity is fully verified. Currently it only checks
    // the Matrix account.
    pub async fn is_fully_verified(&self, net_account: &NetAccount) -> Result<bool> {
        let mut con = self.con.lock().await;
        let transaction = con.transaction()?;

        // Make sure the identity even exists.
        transaction.query_row_named(
            "SELECT
                    id
                FROM
                    pending_judgments
                WHERE
                    net_account = :net_account
                ",
            named_params! {
                ":net_account": net_account,
            },
            |row| row.get::<_, i32>(0),
        )?;

        let is_verified = {
            let mut stmt = transaction.prepare(
                "SELECT
                        id
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
                    AND account_ty_id
                        IN (
                            SELECT
                                id
                            FROM
                                account_types
                            WHERE
                                account_ty IN (
                                    'matrix'
                                )
                        )
                    AND challenge_status_id
                        IN (
                            SELECT
                                id
                            FROM
                                challenge_status
                            WHERE
                                status IN (
                                    'unconfirmed',
                                    'rejected'
                                )
                        )
                    ",
            )?;

            let mut rows = stmt.query_named(named_params! {
                ":net_account": net_account,
            })?;

            rows.next()?.is_none()
        };

        transaction.commit()?;

        Ok(is_verified)
    }
    // TODO: Test this
    pub async fn select_invalid_accounts(
        &self,
        net_account: &NetAccount,
    ) -> Result<Vec<(AccountType, Account)>> {
        let mut con = self.con.lock().await;
        let transaction = con.transaction()?;

        // Make sure the identity even exists.
        transaction.query_row_named(
            "SELECT
                    id
                FROM
                    pending_judgments
                WHERE
                    net_account = :net_account
                ",
            named_params! {
                ":net_account": net_account,
            },
            |row| row.get::<_, i32>(0),
        )?;

        let account_set = {
            let mut stmt = transaction.prepare(
                "
            SELECT
                account_ty, account
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
            AND
                account_status_id
                    IN (
                        SELECT
                            id
                        FROM
                            account_status
                        WHERE
                            status = 'invalid'
                    )
            ",
            )?;

            let mut rows = stmt.query_named(named_params! {
                ":net_account": net_account
            })?;

            let mut account_set = vec![];
            while let Some(row) = rows.next()? {
                account_set.push((row.get::<_, AccountType>(0)?, row.get::<_, Account>(1)?))
            }

            account_set
        };

        transaction.commit()?;

        Ok(account_set)
    }
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
                    twitter_id
                )
            VALUES (
                :account,
                :twitter_id
            )
        ",
        )?;

        for (account, twitter_id) in pair {
            stmt.execute_named(named_params! {
                    ":account": account,
                    ":twitter_id": twitter_id,
            })?;
        }

        Ok(())
    }
    pub async fn select_twitter_id(&self, account: &Account) -> Result<Option<TwitterId>> {
        let con = self.con.lock().await;
        con.query_row_named(
            "
            SELECT
                twitter_id
            FROM
                known_twitter_ids
            WHERE
                account = :account

        ",
            named_params! {
                ":account": account,
            },
            |row| row.get::<_, TwitterId>(0),
        )
        .optional()
        .map_err(|err| failure::Error::from(err))
    }
    pub async fn select_watermark(&self, account_ty: &AccountType) -> Result<u64> {
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
        .map_err(|err| failure::Error::from(err))
        .map(|v| v as u64)
    }
    pub async fn update_watermark(&self, account_ty: &AccountType, value: u64) -> Result<()> {
        let con = self.con.lock().await;
        con.execute_named(
            "
            UPDATE watermarks
            SET watermark = :value
            WHERE
                account_ty_id = (
                    SELECT
                        id
                    FROM
                        accounts_types
                    WHERE
                        account_ty = :account_ty
                )
        ",
            named_params! {
                ":value": value as i64,
                ":account_ty": account_ty,
            },
        )?;

        Ok(())
    }
}

/// A simple abstraction layer over rocksdb. This is used primarily to have a
/// single database object and to create `ScopedDatabase` types, in order to
/// keep data partitioned (with column families).
pub struct Database {
    db: DB,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_missing_column_families(true);
        opts.create_if_missing(true);

        Ok(Database {
            //db: DB::open(&opts, path)?,
            db: DB::open_cf(&opts, path, &["pending_identities", "matrix_rooms"])?,
        })
    }
    pub fn scope<'a>(&'a self, cf_name: &str) -> ScopedDatabase<'a> {
        ScopedDatabase {
            db: &self.db,
            cf_name: cf_name.to_owned(),
        }
    }
}

pub struct ScopedDatabase<'a> {
    db: &'a DB,
    // `ColumnFamily` cannot be shared between threads, so just save it as a String.
    cf_name: String,
}

impl<'a> ScopedDatabase<'a> {
    fn cf(&self) -> Result<&ColumnFamily> {
        Ok(self
            .db
            .cf_handle(&self.cf_name)
            .ok_or(err_msg("fatal error: column family not found"))?)
    }
    pub fn put<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, val: V) -> Result<()> {
        Ok(self.db.put_cf(self.cf()?, key, val)?)
    }
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Result<Option<Vec<u8>>> {
        Ok(self.db.get_cf(self.cf()?, key)?)
    }
    pub fn delete<K: AsRef<[u8]>>(&self, key: K) -> Result<()> {
        Ok(self.db.delete_cf(self.cf()?, key)?)
    }
    pub fn all(&self) -> Result<Vec<(Box<[u8]>, Box<[u8]>)>> {
        Ok(self
            .db
            .iterator_cf(self.cf()?, IteratorMode::Start)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{Challenge, NetAccount};
    use tokio::runtime::Runtime;

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

            println!(">> {:?}", idents);
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
            let eve_room = RoomId::try_from("!EVE:matrix.org").unwrap();

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

            // Get NetAccount based on RoomId.
            let res = db
                .select_net_account_from_room_id(&bob_room)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(res, bob);

            // Does not exist.
            let res = db.select_net_account_from_room_id(&eve_room).await.unwrap();
            assert!(res.is_none());
        });
    }

    #[test]
    fn set_challenge_status() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");

            // Alice does not exist.
            let res = db.is_fully_verified(&alice).await;
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

            // Alice is not verified.
            let res = db.is_fully_verified(&alice).await.unwrap();
            assert_eq!(res, false);

            // Accept an account.
            db.set_challenge_status(&alice, &AccountType::Web, ChallengeStatus::Accepted)
                .await
                .unwrap();

            // Not all essential accounts have been verified yet.
            let res = db.is_fully_verified(&alice).await.unwrap();
            assert_eq!(res, false);

            // Accept an additional account.
            db.set_challenge_status(&alice, &AccountType::Matrix, ChallengeStatus::Accepted)
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
}
