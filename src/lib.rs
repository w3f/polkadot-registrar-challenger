#[macro_use]
extern crate futures;
#[macro_use]
extern crate serde;

use failure::err_msg;
use futures::join;
use rand::{thread_rng, Rng};
use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

use adapters::MatrixClient;
use db::Database;
use identity::{AddressState, CommsMessage, CommsVerifier, IdentityManager, OnChainIdentity};

mod adapters;
mod db;
mod identity;

// TODO: add cfg
struct TestClient {
    comms: CommsVerifier,
}

impl TestClient {
    fn new(comms: CommsVerifier) -> Self {
        TestClient { comms: comms }
    }
    fn gen_data(&self) {
        self.comms.new_on_chain_identity(&OnChainIdentity {
            pub_key: PubKey(SchnorrkelPubKey::default()),
            display_name: None,
            legal_name: None,
            email: None,
            web: None,
            twitter: None,
            matrix: Some(AddressState::new(
                Address("@fabio:web3.foundation".to_string()),
                AddressType::Matrix,
            )),
        });
    }
}

pub struct Config {
    pub db_path: String,
    pub matrix_homeserver: String,
    pub matrix_username: String,
    pub matrix_password: String,
}

pub async fn run(config: Config) -> Result<()> {
    // Setup database and identity manager
    let db = Database::new(&config.db_path)?;
    let mut manager = IdentityManager::new(db)?;

    // Prepare communication channels between manager and clients.
    let c_matrix = manager.register_comms(AddressType::Matrix);
    let c_temp = manager.register_comms(AddressType::Email);

    // Setup clients.
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        &config.matrix_username,
        &config.matrix_password,
        c_matrix,
    )
    .await;

    TestClient::new(c_temp).gen_data();

    println!("Starting all...");
    tokio::spawn(async move {
        manager.start().await;
    });
    tokio::spawn(async move {
        matrix.start().await;
    });

    // TODO: Adjust this
    let mut interval = time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
    }

    Ok(())
}

type Result<T> = StdResult<T, failure::Error>;

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct PubKey(SchnorrkelPubKey);
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Signature(SchnorrkelSignature);
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
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

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
