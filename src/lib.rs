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
use std::fmt::Debug;
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

use adapters::MatrixClient;
use connector::Connector;
use db::Database;
use identity::{AccountState, CommsMessage, CommsVerifier, IdentityManager, OnChainIdentity};

mod adapters;
mod connector;
mod db;
mod identity;
mod verifier;

pub struct Config {
    pub db_path: String,
    pub watcher_url: String,
    pub matrix_homeserver: String,
    pub matrix_username: String,
    pub matrix_password: String,
}

pub async fn run(config: Config) -> Result<()> {
    // Setup database and identity manager
    let db = Database::new(&config.db_path)?;
    let mut manager = IdentityManager::new(db)?;

    // Prepare communication channels between manager and tasks.
    let c_connector = manager.register_comms(AccountType::ReservedConnector);
    let c_emitter = manager.register_comms(AccountType::ReservedEmitter);
    let c_matrix = manager.register_comms(AccountType::Matrix);
    // TODO: move to a test suite
    let c_test = manager.register_comms(AccountType::Email);

    let connector = Connector::new(&config.watcher_url, c_connector).await?;

    // Setup clients.
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        &config.matrix_username,
        &config.matrix_password,
        c_matrix,
        //c_matrix_emitter,
        c_emitter,
    )
    .await?;

    // TODO: move to a test suite
    TestClient::new(c_test).gen_data();

    println!("Starting all...");
    tokio::spawn(async move {
        manager.start().await.unwrap();
    });
    tokio::spawn(async move {
        connector.start().await;
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

impl PubKey {
    fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
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

#[derive(Eq, PartialEq, Clone, Debug)]
pub struct Signature(SchnorrkelSignature);

impl Signature {
    fn to_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }
    fn to_string(&self) -> String {
        hex::encode(&self.to_bytes())
    }
}
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Account(String);

impl Account {
    fn as_str(&self) -> &str {
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

#[derive(Clone)]
pub struct NetworkAddress {
    network_address: String,
    algo: Algorithm,
    pub_key: PubKey,
}

#[derive(Clone, Serialize, Deserialize)]
enum Algorithm {
    #[serde(rename = "schnorr")]
    Schnorr,
    #[serde(rename = "edwards")]
    Edwards,
    #[serde(rename = "ecdsa")]
    ECDSA,
}

impl NetworkAddress {
    fn network_address(&self) -> &str {
        self.network_address.as_str()
    }
    fn algo(&self) -> &Algorithm {
        &self.algo
    }
    fn pub_key(&self) -> &PubKey {
        &self.pub_key
    }
}

use base58::FromBase58;

impl TryFrom<Account> for NetworkAddress {
    type Error = failure::Error;

    fn try_from(value: Account) -> Result<Self> {
        let bytes = value
            .0
            .from_base58()
            .map_err(|_| err_msg("failed to decode address from base58"))?;

        if bytes.len() < 33 {
            return Err(err_msg("invalid address"));
        }

        Ok(NetworkAddress {
            network_address: value.0,
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

trait Fatal<T> {
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
