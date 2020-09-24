use super::Result;
use crate::identity::{AccountStatus, OnChainIdentity};
use crate::primitives::{AccountType, NetAccount};
use failure::err_msg;
use matrix_sdk::identifiers::RoomId;
use rocksdb::{ColumnFamily, IteratorMode, Options, DB};
use rusqlite::{named_params, params, Connection};
use std::convert::AsRef;
use std::sync::Arc;

#[derive(Debug, Fail)]
pub enum DatabaseError {
    #[fail(display = "failed to open SQLite database: {}", 0)]
    Open(failure::Error),
    #[fail(display = "SQLite database is not in auto-commit mode")]
    NoAutocommit,
}

#[derive(Clone)]
pub struct Database2 {
    con: Arc<Connection>,
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
                id          INTEGER PRIMARY KEY,
                address     TEXT NOT NULL,
                legal_name  TEXT,
                email       TEXT,
                web         TEXT,
                twitter     TEXT,
                matrix      TEXT
            )",
                table = PENDING_JUDGMENTS,
            ),
            params![],
        )?;

        // Table for account status.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                    id        INTEGER PRIMARY KEY,
                    status  TEXT NOT NULL
            )",
                table = ACCOUNT_STATUS
            ),
            params![],
        )?;

        // Table for challenge status.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                    id        INTEGER PRIMARY KEY,
                    status  TEXT NOT NULL
            )",
                table = CHALLENGE_STATUS
            ),
            params![],
        )?;

        // Table for account type.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                    id          INTEGER PRIMARY KEY,
                    account_ty  TEXT NOT NULL
            )",
                table = ACCOUNT_TYPES
            ),
            params![],
        )?;

        // Table for account state.
        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table_main} (
                id                  INTEGER PRIMARY KEY,
                address_id          INTEGER NOT NULL,
                account             TEXT NOT NULL,
                account_ty          INTEGER NOT NULL,
                account_status      INTEGER NOT NULL,
                challenge           TEXT NOT NULL,
                challenge_status    INTEGER NOT NULL,

                FOREIGN KEY (address_id)
                    REFERENCES {table_judgments} (id),

                FOREIGN KEY (account_ty)
                    REFERENCES {table_account_ty} (id),

                FOREIGN KEY (account_status)
                    REFERENCES {table_account_status} (id),

                FOREIGN KEY (challenge_status)
                    REFERENCES {table_challenge_status} (id)
            )",
                table_main = ACCOUNT_STATE,
                table_judgments = PENDING_JUDGMENTS,
                table_account_ty = ACCOUNT_TYPES,
                table_account_status = ACCOUNT_STATUS,
                table_challenge_status = CHALLENGE_STATUS,
            ),
            params![],
        )?;

        con.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {table} (
                id          INTEGER PRIMARY KEY,
                address_id  INTEGER NULL,
                room_id     TEXT,

                FOREIGN KEY (address_id)
                    REFERENCES pending_judgments (id)
            )",
                table = KNOWN_MATRIX_ROOMS,
            ),
            params![],
        )?;

        Ok(Database2 { con: Arc::new(con) })
    }
    pub fn insert_identity(&self, ident: &OnChainIdentity) -> Result<()> {
        self.insert_identity_batch(&[ident])
    }
    pub fn insert_identity_batch(&self, idents: &[&OnChainIdentity]) -> Result<()> {
        let mut stmt = self.con.prepare(&format!(
            "INSERT INTO {table} (
                address,
                display_name,
                legal_name,
                email,
                web,
                twitter,
                matrix
            ) VALUES (
                ':address,'
                ':display_name,'
                ':legal_name,'
                ':email,'
                ':web,'
                ':twitter:'
                ':matrix'
            )",
            table = PENDING_JUDGMENTS
        ))?;

        for ident in idents {
            stmt.execute_named(named_params! {
                ":address": ident.network_address.address().as_str(),
                ":display_name": ident.display_name,
                ":legal_name": ident.legal_name,
                ":email": ident.email.as_ref().map(|s| s.account_str()),
                ":web": ident.web.as_ref().map(|s| s.account_str()),
                ":twitter": ident.twitter.as_ref().map(|s| s.account_str()),
                ":matrix": ident.matrix.as_ref().map(|s| s.account_str()),
            })?;
        }

        Ok(())
    }
    pub fn insert_room_id(&self, net_account: NetAccount, room_id: &RoomId) -> Result<()> {
        self.con.execute_named(
            &format!(
                "INSERT INTO {table_into} (
                    address_id,
                    room_id
                ) VALUES (
                        (SELECT id FROM {table_from} WHERE address = ':account'),
                        ':room_id'
                    )
                )",
                table_into = KNOWN_MATRIX_ROOMS,
                table_from = PENDING_JUDGMENTS,
            ),
            named_params! {
                ":account": net_account.as_str(),
                ":room_id": room_id.as_str(),
            },
        )?;

        Ok(())
    }
    pub fn set_account_status(
        &self,
        net_account: NetAccount,
        account_ty: AccountType,
        status: AccountStatus,
    ) -> Result<()> {
        self.con.execute_named(
            &format!(
            "UPDATE {tbl_update}
                SET account_status =
                    (SELECT id FROM {tbl_account_status}
                        WHERE status = ':account_status')
                WHERE
                    net_account_id =
                        (SELECT id FROM {tbl_identities}
                            WHERE address = ':net_account')
                AND
                    account_ty =
                        (SELECT id FROM {tbl_acc_types}
                            WHERE account_ty = ':account_ty')
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

    #[test]
    fn database_setup() {
        let db = Database2::new("/tmp/sqlite").unwrap();
    }
}
