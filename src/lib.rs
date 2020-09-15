#[macro_use]
extern crate futures;
#[macro_use]
extern crate serde;

use failure::err_msg;
use rand::{thread_rng, Rng};
use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;
use std::result::Result as StdResult;
use futures::join;

use adapters::MatrixClient;
use db::Database;
use identity::IdentityManager;

mod adapters;
mod db;
mod identity;

pub struct Config {
    pub db_path: String,
    pub matrix_homeserver: String,
    pub matrix_username: String,
    pub matrix_password: String,
}

pub async fn run(config: Config) -> Result<()> {
    // Setup database and identity manager
    let db = Database::new(&config.db_path)?;
    let mut manager = IdentityManager::new(&db)?;

    // Prepare communication channels between manager and clients.
    let c_matrix = manager.register_comms(AddressType::Matrix);

    // Setup clients.
    let mut matrix = MatrixClient::new(
        &config.matrix_homeserver,
        &config.matrix_username,
        &config.matrix_password,
        c_matrix,
    ).await;

    join!(
        manager.start(),
        matrix.start()
    );

    Ok(())
}

type Result<T> = StdResult<T, failure::Error>;

#[derive(Eq, PartialEq, Clone)]
pub struct PubKey(SchnorrkelPubKey);
#[derive(Eq, PartialEq, Clone)]
pub struct Signature(SchnorrkelSignature);
#[derive(Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Address(String);

#[derive(Clone)]
///
pub struct RoomId(String);

impl TryFrom<Vec<u8>> for RoomId {
    type Error = failure::Error;

    fn try_from(value: Vec<u8>) -> Result<Self> {
        Ok(RoomId(
            String::from_utf8(value).map_err(|_| err_msg("invalid room id"))?,
        ))
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
