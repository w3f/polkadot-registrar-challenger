#![recursion_limit = "512"]

#[macro_use]
extern crate log;
#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate failure;

use adapters::MatrixClient;
use comms::CommsVerifier;
use connector::Connector;
use db::Database;
use identity::IdentityManager;
use primitives::{AccountType, Fatal, Result};
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::process::exit;
use tokio::time::{self, Duration};

pub mod adapters;
mod comms;
pub mod connector;
mod db;
mod identity;
mod primitives;
mod verifier;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub registrar_db_path: String,
    pub matrix_db_path: String,
    pub log_level: log::LevelFilter,
    pub watcher_url: String,
    pub enable_watcher: bool,
    //
    pub matrix_homeserver: String,
    pub matrix_username: String,
    pub matrix_password: String,
    //
    pub twitter_api_key: String,
    pub twitter_api_secret: String,
    pub twitter_token: String,
    pub twitter_token_secret: String,
    //
    pub google_private_key: String,
    pub google_issuer: String,
    pub google_scope: String,
    pub google_email: String,
}

fn open_config() -> Result<Config> {
    // Open config file.
    let mut file = File::open("config.json")
        .or_else(|_| File::open("/etc/registrar/config.json"))
        .map_err(|_| {
            eprintln!("Failed to open config at 'config.json' or '/etc/registrar/config.json'.");
            std::process::exit(1);
        })
        .fatal();

    // Parse config file as JSON.
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let config = serde_json::from_str::<Config>(&contents)
        .map_err(|err| {
            eprintln!("Failed to parse config: {}", err);
            std::process::exit(1);
        })
        .fatal();

    Ok(config)
}

pub fn init_env() -> Result<Config> {
    let config = open_config()?;

    // Env variables for log level overwrites config.
    if let Ok(_) = env::var("RUST_LOG") {
        println!("Env variable 'RUST_LOG' found, overwriting logging level from config.");
        env_logger::init();
    } else {
        println!("Setting log level to '{}' from config.", config.log_level);
        env_logger::builder()
            .filter_module("registrar", config.log_level)
            .init();
    }

    println!("Logger initiated");

    Ok(config)
}

pub async fn block() {
    let mut interval = time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
    }
}

pub async fn run(config: Config) -> Result<()> {
    setup(config).await.map(|_| ())
}

pub async fn run_with_feeder(config: Config) -> Result<CommsVerifier> {
    setup(config).await
}

pub async fn setup(config: Config) -> Result<CommsVerifier> {
    info!("Setting up database and manager");
    let db = Database::new(&config.registrar_db_path)?;
    let mut manager = IdentityManager::new(db)?;

    info!("Setting up communication channels");
    let c_connector = manager.register_comms(AccountType::ReservedConnector);
    let c_emitter = manager.register_comms(AccountType::ReservedEmitter);
    let c_matrix = manager.register_comms(AccountType::Matrix);
    let c_feeder = manager.register_comms(AccountType::ReservedFeeder);

    info!("Trying to connect to Watcher");
    let mut counter = 0;
    let mut interval = time::interval(Duration::from_secs(5));

    let mut connector = None;
    loop {
        interval.tick().await;

        // Only connect to Watcher if the config specifies so.
        if !config.enable_watcher {
            break;
        }

        if let Ok(con) = Connector::new(&config.watcher_url, c_connector.clone()).await {
            info!("Connecting to Watcher succeeded");
            connector = Some(con);
            break;
        } else {
            warn!("Connecting to Watcher failed, trying again...");
        }

        if counter == 2 {
            error!("Failed connecting to Watcher, exiting...");
            exit(1);
        }

        counter += 1;
    }

    info!("Starting manager task");
    tokio::spawn(async move {
        manager.start().await;
    });

    info!("Setting up Matrix client");
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        &config.matrix_username,
        &config.matrix_password,
        &config.matrix_db_path,
        c_matrix,
        c_emitter,
    )
    .await?;

    info!("Starting Matrix task");
    tokio::spawn(async move {
        matrix.start().await;
    });

    if config.enable_watcher {
        info!("Starting Watcher connector task, listening...");
        tokio::spawn(async move {
            connector.unwrap().start().await;
        });
    } else {
        warn!("Watcher connector task is disabled. Cannot process any requests...");
    }

    Ok(c_feeder)
}
