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
use base58::FromBase58;

use adapters::MatrixClient;
use connector::Connector;
use db::Database;
use identity::{AccountState, CommsMessage, CommsVerifier, IdentityManager, OnChainIdentity};
use primitives::{Account, NetAccount, AccountType, Challenge, Fatal, PubKey, Result, NetworkAddress, Algorithm};
use identity::TestClient;

mod adapters;
mod connector;
mod db;
mod identity;
mod verifier;
mod primitives;

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
