use base58::FromBase58;
use failure::err_msg;

use rand::{thread_rng, Rng};
use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;
use std::fmt::Debug;
use std::result::Result as StdResult;


pub type Result<T> = StdResult<T, failure::Error>;

#[derive(Eq, PartialEq, Clone, Debug)]
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

impl Signature {
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }
    pub fn to_string(&self) -> String {
        hex::encode(&self.to_bytes())
    }
}

impl From<SchnorrkelSignature> for Signature {
    fn from(value: SchnorrkelSignature) -> Self {
        Signature(value)
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct NetAccount(String);

impl NetAccount {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
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

#[derive(Debug, Clone, Serialize, Deserialize)]
// TODO: Make fields private
pub struct NetworkAddress {
    pub address: NetAccount,
    pub algo: Algorithm,
    pub pub_key: PubKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn algo(&self) -> &Algorithm {
        &self.algo
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
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "web")]
    Web,
    #[serde(rename = "twitter")]
    Twitter,
    #[serde(rename = "matrix")]
    Matrix,
    #[serde(rename = "reserved_connector")]
    ReservedConnector,
    #[serde(rename = "reserved_emitter")]
    ReservedEmitter,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Challenge(String);

impl Challenge {
    pub fn gen_random() -> Challenge {
        let random: [u8; 16] = thread_rng().gen();
        Challenge(hex::encode(random))
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
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

pub trait Fatal<T> {
    fn fatal(self) -> T;
}

impl<T: Debug, E: Debug> Fatal<T> for StdResult<T, E> {
    fn fatal(self) -> T {
        if self.is_err() {
            let err = self.unwrap_err();
            // TODO: log
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
