use super::Result;
#[cfg(test)]
use crate::connector::{Accounts, JudgementRequest};
use crate::identity::{AccountStatus, OnChainIdentity};
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, Fatal, NetAccount, PubKey,
};
use failure::err_msg;
use matrix_sdk::identifiers::RoomId;
use rocksdb::{ColumnFamily, IteratorMode, Options, DB};
use rusqlite::{named_params, params, Connection, OptionalExtension};
use std::convert::{AsRef, TryFrom};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Fail)]
pub enum DatabaseError {
    #[fail(display = "failed to open SQLite database: {}", 0)]
    Open(failure::Error),
    #[fail(display = "SQLite database is not in auto-commit mode")]
    NoAutocommit,
    #[fail(display = "failed to convert column field into native type")]
    InvalidType,
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

        // Table for known matrix rooms.
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
                    ":net_account": ident.network_address.address()
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

            // TODO -> Use a HashMap for OnChainIdentity regarding accounts.
            // TODO: Support more than just matrix.
            for ident in idents {
                if ident.matrix.is_none() {
                    continue;
                }
                stmt.execute_named(named_params! {
                    ":net_account": ident.network_address.address(),
                    ":account": ident.matrix.as_ref().map(|s| &s.account),
                    ":account_ty": ident.matrix.as_ref().map(|s| &s.account_ty),
                    ":account_status": ident.matrix.as_ref().map(|s| &s.account_status),
                    ":challenge": ident.matrix.as_ref().map(|s| s.challenge.as_str()),
                    ":challenge_status": ident.matrix.as_ref().map(|s| &s.challenge_status),
                })?;
            }
        }

        transaction.commit()?;

        Ok(())
    }
    #[cfg(test)]
    async fn select_identities(&self) -> Result<Vec<JudgementRequest>> {
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

        let mut idents: Vec<JudgementRequest> = vec![];

        // TODO: Support more than just matrix.
        let mut rows = stmt.query(params![])?;
        while let Some(row) = rows.next()? {
            let net_account = NetAccount::from(row.get::<_, String>(0)?);

            // let account_ty = rows.get::<_, AccountType>(1)?;

            if let Some(request) = idents.iter_mut().find(|req| req.address == net_account) {
                request.accounts.matrix = Some(Account::from(row.get::<_, String>(2)?));
            } else {
                idents.push(JudgementRequest {
                    address: net_account,
                    accounts: Accounts {
                        display_name: None,
                        legal_name: None,
                        email: None,
                        web: None,
                        twitter: None,
                        matrix: Some(Account::from(row.get::<_, String>(2)?)),
                    },
                });
            }
        }

        Ok(idents)
    }
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
    pub async fn set_account_status(
        &self,
        net_account: &NetAccount,
        account_ty: AccountType,
        status: AccountStatus,
    ) -> Result<()> {
        self.con.lock().await.execute_named(
            &format!(
                "UPDATE {tbl_update}
                SET account_status =
                    (SELECT id FROM {tbl_account_status}
                        WHERE status = :account_status)
                WHERE
                    net_account_id =
                        (SELECT id FROM {tbl_identities}
                            WHERE address = :net_account)
                AND
                    account_ty =
                        (SELECT id FROM {tbl_acc_types}
                            WHERE account_ty = :account_ty)
            )",
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
        )?;

        Ok(())
    }
    pub async fn set_challenge_status(
        &self,
        net_account: &NetAccount,
        account_ty: AccountType,
        status: ChallengeStatus,
    ) -> Result<()> {
        self.con.lock().await.execute_named(
            &format!(
                "UPDATE {tbl_update}
                SET challenge_status =
                    (SELECT id FROM {tbl_challenge_status}
                        WHERE status = :challenge_status)
                WHERE
                    net_account_id =
                        (SELECT id FROM {tbl_identities}
                            WHERE address = :net_account)
                AND
                    account_ty =
                        (SELECT id FROM {tbl_acc_types}
                            WHERE account_ty = :account_ty)
            )",
                tbl_update = ACCOUNT_STATE,
                tbl_challenge_status = CHALLENGE_STATUS,
                tbl_identities = PENDING_JUDGMENTS,
                tbl_acc_types = ACCOUNT_TYPES,
            ),
            named_params! {
                ":account_status": status,
                ":net_account": net_account,
                ":account_ty": account_ty,
            },
        )?;

        Ok(())
    }
    pub async fn select_challenge_data(
        &self,
        account: &Account,
        account_ty: AccountType,
    ) -> Result<(NetAccount, PubKey, Challenge)> {
        Err(failure::err_msg(""))
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
    use crate::connector::{Accounts, JudgementRequest};
    use crate::primitives::{NetAccount, Challenge};
    use std::convert::TryInto;
    use tokio::runtime::Runtime;

    // Generate a random db path
    fn db_path() -> String {
        format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
    }

    #[test]
    fn database_setup() {
        let path = db_path();

        // Test repeated initialization.
        let db = Database2::new(&path).unwrap();
        let db = Database2::new(&path).unwrap();
    }

    #[test]
    fn insert_identity() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            let mut request = JudgementRequest {
                address: NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU"),
                accounts: Accounts {
                    display_name: Some("Alice".to_string()),
                    legal_name: None,
                    email: Some(Account::from("alice@email.com")),
                    web: None,
                    twitter: Some(Account::from("twitter.com/alice")),
                    matrix: Some(Account::from("@alice:matrix.org")),
                },
            };

            // Insert and check return value.
            let _ = db
                .insert_identity(&request.clone().try_into().unwrap())
                .await
                .unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(
                res[0].accounts.matrix.as_ref().unwrap(),
                &Account::from("@alice:matrix.org")
            );

            // Repeated insert of same value.
            let _ = db
                .insert_identity(&request.clone().try_into().unwrap())
                .await
                .unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(
                res[0].accounts.matrix.as_ref().unwrap(),
                &Account::from("@alice:matrix.org")
            );

            // Change a field, insert and return value.
            request.accounts.matrix = Some(Account::from("@bob:matrix.org"));

            let _ = db
                .insert_identity(&request.try_into().unwrap())
                .await
                .unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 1);
            assert_eq!(
                res[0].accounts.matrix.as_ref().unwrap(),
                &Account::from("@bob:matrix.org")
            );
        });
    }

    #[test]
    fn insert_identity_batch() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            let requests = vec![
                // Two identical identities with the same values.
                JudgementRequest {
                    address: NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU"),
                    accounts: Accounts {
                        display_name: Some("Alice".to_string()),
                        legal_name: None,
                        email: Some(Account::from("alice@email.com")),
                        web: None,
                        twitter: Some(Account::from("twitter.com/alice")),
                        matrix: Some(Account::from("@alice:matrix.org")),
                    },
                }
                .try_into()
                .unwrap(),
                JudgementRequest {
                    address: NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU"),
                    accounts: Accounts {
                        display_name: Some("Alice".to_string()),
                        legal_name: None,
                        email: Some(Account::from("alice@email.com")),
                        web: None,
                        twitter: Some(Account::from("twitter.com/alice")),
                        matrix: Some(Account::from("@alice:matrix.org")),
                    },
                }
                .try_into()
                .unwrap(),
                // Two identical identities with varying values (matrix).
                JudgementRequest {
                    address: NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C"),
                    accounts: Accounts {
                        display_name: Some("Bob".to_string()),
                        legal_name: None,
                        email: Some(Account::from("bob@email.com")),
                        web: None,
                        twitter: Some(Account::from("twitter.com/bob")),
                        matrix: Some(Account::from("@bob:matrix.org")),
                    },
                }
                .try_into()
                .unwrap(),
                JudgementRequest {
                    address: NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C"),
                    accounts: Accounts {
                        display_name: Some("Bob".to_string()),
                        legal_name: None,
                        email: Some(Account::from("bob@email.com")),
                        web: None,
                        twitter: Some(Account::from("twitter.com/bob")),
                        matrix: Some(Account::from("@bob_second:matrix.org")),
                    },
                }
                .try_into()
                .unwrap(),
            ];

            let requests: Vec<&OnChainIdentity> = requests.iter().map(|ident| ident).collect();

            let _ = db.insert_identity_batch(&requests).await.unwrap();

            let res = db.select_identities().await.unwrap();
            assert_eq!(res.len(), 2);
            res.iter()
                .find(|request| {
                    request.address
                        == NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU")
                })
                .map(|request| {
                    assert_eq!(
                        request.accounts.matrix.as_ref().unwrap(),
                        &Account::from("@alice:matrix.org")
                    );
                    Some(request)
                })
                .unwrap();

            res.iter()
                .find(|request| {
                    request.address
                        == NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C")
                })
                .map(|request| {
                    assert_eq!(
                        request.accounts.matrix.as_ref().unwrap(),
                        &Account::from("@bob_second:matrix.org")
                    );
                    Some(request)
                })
                .unwrap();
        });
    }

    #[test]
    fn insert_select_room_id() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut db = Database2::new(&db_path()).unwrap();

            // Prepare identities.
            let alice = NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU");
            let bob = NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C");
            let eve = NetAccount::from("13gjXZKFPCELoVN56R2KopsNKAb6xqHwaCfWA8m4DG4s9xGQ");

            let mut request = JudgementRequest {
                address: alice.clone(),
                accounts: Accounts::default(),
            };

            // Insert and check return value.
            let _ = db
                .insert_identity(&request.clone().try_into().unwrap())
                .await
                .unwrap();

            let mut request = JudgementRequest {
                address: bob.clone(),
                accounts: Accounts::default(),
            };

            // Insert and check return value.
            let _ = db
                .insert_identity(&request.clone().try_into().unwrap())
                .await
                .unwrap();

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
}
