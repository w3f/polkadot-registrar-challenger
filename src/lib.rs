#[macro_use]
extern crate futures;
#[macro_use]
extern crate serde;

use rand::{thread_rng, Rng};
use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::result::Result as StdResult;
use std::convert::TryFrom;
use failure::err_msg;

pub mod adapters;
pub mod db;
pub mod identity;

type Result<T> = StdResult<T, failure::Error>;

#[derive(Eq, PartialEq, Clone)]
pub struct PubKey(SchnorrkelPubKey);
#[derive(Eq, PartialEq, Clone)]
pub struct Signature(SchnorrkelSignature);
#[derive(Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Address(String);

#[derive(Clone)]
pub struct RoomId(String);

impl TryFrom<Vec<u8>> for RoomId {
    type Error = failure::Error;

    fn try_from(value: Vec<u8>) -> Result<Self> {
        Ok(RoomId(String::from_utf8(value).map_err(|_| err_msg("invalid room id"))?))
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
pub enum AddressType {
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "web")]
    Web,
    #[serde(rename = "twitter")]
    Twitter,
    #[serde(rename = "matrix")]
    Matrix,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Challenge(String);

impl Challenge {
    fn gen_random() -> Challenge {
        let random: [u8; 16] = thread_rng().gen();
        Challenge(hex::encode(random))
    }
    pub fn verify_challenge(&self, pub_key: &PubKey, sig: &Signature) -> bool {
        pub_key
            .0
            .verify_simple(b"", self.0.as_bytes(), &sig.0)
            .is_ok()
    }
}

impl Serialize for PubKey {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0.to_bytes()))
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
