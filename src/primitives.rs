use base58::FromBase58;
use failure::err_msg;

use rand::{thread_rng, Rng};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use base58::ToBase58;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Display};
use std::result::Result as StdResult;
use std::time::{SystemTime, UNIX_EPOCH};

pub type Result<T> = StdResult<T, failure::Error>;

pub fn unix_time() -> u64 {
    let start = SystemTime::now();
    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PubKey(SchnorrkelPubKey);

impl PubKey {
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

impl From<SchnorrkelPubKey> for PubKey {
    fn from(value: SchnorrkelPubKey) -> Self {
        PubKey(value)
    }
}

impl TryFrom<Vec<u8>> for PubKey {
    type Error = failure::Error;

    fn try_from(value: Vec<u8>) -> Result<Self> {
        Ok(PubKey(
            SchnorrkelPubKey::from_bytes(&value).map_err(|_| err_msg("invalid public key"))?,
        ))
    }
}

impl Serialize for PubKey {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for PubKey {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = <String as Deserialize>::deserialize(deserializer)?;
        Ok(PubKey(
            SchnorrkelPubKey::from_bytes(
                &hex::decode(hex_str)
                    .map_err(|_| SerdeError::custom("failed to decode public key from hex"))?,
            )
            .map_err(|_| SerdeError::custom("failed creating public key from bytes"))?,
        ))
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Signature(SchnorrkelSignature);

impl From<SchnorrkelSignature> for Signature {
    fn from(value: SchnorrkelSignature) -> Self {
        Signature(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct NetAccount(String);

#[cfg(test)]
impl From<&SchnorrkelPubKey> for NetAccount {
    fn from(value: &SchnorrkelPubKey) -> Self {
        // The address here is technically invalid, but it contains enough
        // information in order to extract a public key out of it. So for
        // testing this is sufficient.
        NetAccount::from(format!("1{}", value.to_bytes().to_base58()))
    }
}

impl ToSql for NetAccount {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        use ToSqlOutput::*;
        use ValueRef::*;

        Ok(Borrowed(Text(self.as_str().as_bytes())))
    }
}

impl FromSql for NetAccount {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(val) => Ok(NetAccount(
                String::from_utf8(val.to_vec()).map_err(|_| FromSqlError::InvalidType)?,
            )),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl NetAccount {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    #[cfg(test)]
    pub fn alice() -> Self {
        NetAccount::from("14GcE3qBiEnAyg2sDfadT3fQhWd2Z3M59tWi1CvVV8UwxUfU")
    }
    #[cfg(test)]
    pub fn bob() -> Self {
        NetAccount::from("163AnENMFr6k4UWBGdHG9dTWgrDmnJgmh3HBBZuVWhUTTU5C")
    }
    #[cfg(test)]
    pub fn eve() -> Self {
        NetAccount::from("13gjXZKFPCELoVN56R2KopsNKAb6xqHwaCfWA8m4DG4s9xGQ")
    }
}

impl From<String> for NetAccount {
    fn from(value: String) -> Self {
        NetAccount(value)
    }
}

impl From<&str> for NetAccount {
    fn from(value: &str) -> Self {
        NetAccount(value.to_owned())
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Account(String);

impl FromSql for Account {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(val) => Ok(Account(
                String::from_utf8(val.to_vec()).map_err(|_| FromSqlError::InvalidType)?,
            )),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl ToSql for Account {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        use ToSqlOutput::*;
        use ValueRef::*;

        Ok(Borrowed(Text(self.as_str().as_bytes())))
    }
}

impl Account {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for Account {
    fn from(value: String) -> Self {
        Account(value)
    }
}

impl From<&str> for Account {
    fn from(value: &str) -> Self {
        Account(value.to_owned())
    }
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct NetworkAddress {
    address: NetAccount,
    algo: Algorithm,
    pub_key: PubKey,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Algorithm {
    #[serde(rename = "schnorr")]
    Schnorr,
    #[serde(rename = "edwards")]
    Edwards,
    #[serde(rename = "ecdsa")]
    ECDSA,
}

impl NetworkAddress {
    pub fn address(&self) -> &NetAccount {
        &self.address
    }
    pub fn pub_key(&self) -> &PubKey {
        &self.pub_key
    }
}

impl TryFrom<NetAccount> for NetworkAddress {
    type Error = failure::Error;

    fn try_from(value: NetAccount) -> Result<Self> {
        let bytes = value
            .as_str()
            .from_base58()
            .map_err(|_| err_msg("failed to decode address from base58"))?;

        if bytes.len() < 33 {
            return Err(err_msg("invalid address"));
        }

        Ok(NetworkAddress {
            address: value,
            algo: Algorithm::Schnorr,
            pub_key: PubKey::try_from(bytes[1..33].to_vec())?,
        })
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum AccountType {
    #[serde(rename = "legal_name")]
    LegalName,
    #[serde(rename = "display_name")]
    DisplayName,
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "web")]
    Web,
    #[serde(rename = "twitter")]
    Twitter,
    #[serde(rename = "matrix")]
    Matrix,
    // Reserved types for internal communication.
    //
    // Websocket connection to Watcher
    ReservedConnector,
    // Matrix emitter which reacts on Matrix messages
    ReservedEmitter,
}

impl Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use AccountType::*;

        match self {
            LegalName => write!(f, "Legal Name"),
            DisplayName => write!(f, "Display Name"),
            Email => write!(f, "Email"),
            Web => write!(f, "Web"),
            Twitter => write!(f, "Twitter"),
            Matrix => write!(f, "Matrix"),
            _ => Err(fmt::Error),
        }
    }
}

impl ToSql for AccountType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        use AccountType::*;
        use ToSqlOutput::*;
        use ValueRef::*;

        match self {
            LegalName => Ok(Borrowed(Text(b"legal_name"))),
            DisplayName => Ok(Borrowed(Text(b"display_name"))),
            Email => Ok(Borrowed(Text(b"email"))),
            Web => Ok(Borrowed(Text(b"web"))),
            Twitter => Ok(Borrowed(Text(b"twitter"))),
            Matrix => Ok(Borrowed(Text(b"matrix"))),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

impl FromSql for AccountType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(val) => match val {
                b"legal_name" => Ok(AccountType::LegalName),
                b"display_name" => Ok(AccountType::DisplayName),
                b"email" => Ok(AccountType::Email),
                b"web" => Ok(AccountType::Web),
                b"twitter" => Ok(AccountType::Twitter),
                b"matrix" => Ok(AccountType::Matrix),
                _ => Err(FromSqlError::InvalidType),
            },
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChallengeStatus {
    #[serde(rename = "unconfirmed")]
    Unconfirmed,
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "rejected")]
    Rejected,
}

impl ToSql for ChallengeStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        use ChallengeStatus::*;
        use ToSqlOutput::*;
        use ValueRef::*;

        match self {
            Unconfirmed => Ok(Borrowed(Text(b"unconfirmed"))),
            Accepted => Ok(Borrowed(Text(b"accepted"))),
            Rejected => Ok(Borrowed(Text(b"rejected"))),
        }
    }
}

impl FromSql for ChallengeStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(val) => match val {
                b"unconfirmed" => Ok(ChallengeStatus::Unconfirmed),
                b"accepted" => Ok(ChallengeStatus::Accepted),
                b"rejected" => Ok(ChallengeStatus::Rejected),
                _ => Err(FromSqlError::InvalidType),
            },
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Challenge(pub String);

impl Challenge {
    pub fn gen_random() -> Challenge {
        let random: [u8; 16] = thread_rng().gen();
        Challenge(hex::encode(random))
    }
    #[cfg(test)]
    pub fn gen_fixed() -> Challenge {
        let data: [u8; 16] = [1; 16];
        Challenge(hex::encode(data))
    }
    pub fn verify_challenge(&self, pub_key: &PubKey, sig: &Signature) -> bool {
        pub_key
            .0
            .verify_simple(b"substrate", self.0.as_bytes(), &sig.0)
            .is_ok()
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Judgement {
    #[serde(rename = "reasonable")]
    Reasonable,
    #[serde(rename = "erroneous")]
    Erroneous,
}

pub trait Fatal<T> {
    fn fatal(self) -> T;
}

impl<T: Debug, E: Debug> Fatal<T> for StdResult<T, E> {
    fn fatal(self) -> T {
        if self.is_err() {
            let err = self.unwrap_err();
            panic!("Fatal error encountered. Report as a bug: {:?}", err);
        }

        self.unwrap()
    }
}

impl<T: Debug> Fatal<T> for Option<T> {
    fn fatal(self) -> T {
        self.expect("Fatal error encountered. Report as a bug.")
    }
}
