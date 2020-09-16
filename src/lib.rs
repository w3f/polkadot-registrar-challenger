#[macro_use]
extern crate futures;
#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate failure;

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
use identity::{AccountState, CommsMessage, CommsVerifier, IdentityManager, OnChainIdentity};

mod adapters;
mod db;
mod identity;
mod listener;
mod verifier;

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
    let c_matrix = manager.register_comms(AccountType::Matrix);
    let c_temp = manager.register_comms(AccountType::Email);

    // Prepare special-purpose communication channels.
    let c_matrix_emitter = manager.emitter_comms();

    // Setup clients.
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        &config.matrix_username,
        &config.matrix_password,
        c_matrix,
        c_matrix_emitter,
    )
    .await;

    TestClient::new(c_temp).gen_data();

    println!("Starting all...");
    tokio::spawn(async move {
        manager.start().await.unwrap();
    });
    tokio::spawn(async move {
        matrix.start().await.unwrap();
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

impl TryFrom<Vec<u8>> for PubKey {
    type Error = failure::Error;

    fn try_from(value: Vec<u8>) -> Result<Self> {
        Ok(PubKey(
            SchnorrkelPubKey::from_bytes(&value).map_err(|_| err_msg("invalid public key"))?,
        ))
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Signature(SchnorrkelSignature);
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Account(String);

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

#[derive(Clone)]
/// TODO: Just use Account
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
pub enum AccountType {
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
            .verify_simple(b"substrate", self.0.as_bytes(), &sig.0)
            .is_ok()
    }
}

// TODO: add cfg
struct TestClient {
    comms: CommsVerifier,
}

impl TestClient {
    fn new(comms: CommsVerifier) -> Self {
        TestClient { comms: comms }
    }
    fn gen_data(&self) {
        let sk = schnorrkel::keys::SecretKey::generate();
        let pk = sk.to_public();

        let mut ident = OnChainIdentity {
            //pub_key: PubKey(SchnorrkelPubKey::default()),
            pub_key: PubKey(pk),
            display_name: None,
            legal_name: None,
            email: None,
            web: None,
            twitter: None,
            matrix: Some(AccountState::new(
                Account("@fabio:web3.foundation".to_string()),
                AccountType::Matrix,
            )),
        };

        let random = &ident.matrix.as_ref().unwrap().challenge;

        let sig = sk.sign_simple(b"substrate", &random.0.as_bytes(), &pk);
        println!("SIG: >> {}", hex::encode(&sig.to_bytes()));

        self.comms.new_on_chain_identity(&ident);
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
