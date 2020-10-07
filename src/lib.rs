#[macro_use]
extern crate log;
#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate failure;

use adapters::{ClientBuilder, MatrixClient, TwitterBuilder};
use connector::Connector;
use db::Database2;
use health_check::HealthCheck;
use identity::IdentityManager;
use primitives::{Account, AccountType, Fatal, Result};
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::process::exit;
use tokio::time::{self, Duration};

pub mod adapters;
mod comms;
pub mod connector;
pub mod db;
mod health_check;
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
    pub disable_tasks: bool,
    //
    pub matrix_homeserver: String,
    pub matrix_username: String,
    pub matrix_password: String,
    //
    pub twitter_screen_name: String,
    pub twitter_api_key: String,
    pub twitter_api_secret: String,
    pub twitter_token: String,
    pub twitter_token_secret: String,
    //
    pub email_server: String,
    pub imap_server: String,
    pub email_inbox: String,
    pub email_user: String,
    pub email_password: String,
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

pub async fn setup(config: Config) -> Result<()> {
    info!("Setting up database and manager");
    let db2 = Database2::new(&config.registrar_db_path)?;
    let mut manager = IdentityManager::new(db2.clone())?;

    info!("Setting up communication channels");
    let c_connector = manager.register_comms(AccountType::ReservedConnector);
    let c_emitter = manager.register_comms(AccountType::ReservedEmitter);
    let c_matrix = manager.register_comms(AccountType::Matrix);
    let c_twitter = manager.register_comms(AccountType::Twitter);
    let c_email = manager.register_comms(AccountType::Email);

    info!("Starting manager task");
    tokio::spawn(async move {
        manager.start().await;
    });

    info!("Starting health check thread");
    std::thread::spawn(|| {
        HealthCheck::start()
            .map_err(|err| {
                error!("Failed to start health check service: {}", err);
                std::process::exit(1);
            })
            .unwrap();
    });

    if config.disable_tasks {
        return Ok(())
    }

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

    info!("Setting up Matrix client");
    let matrix = MatrixClient::new(
        &config.matrix_homeserver,
        &config.matrix_username,
        &config.matrix_password,
        &config.matrix_db_path,
        db2.clone(),
        c_matrix,
        c_emitter,
    )
    .await?;

    info!("Setting up Twitter client");
    let twitter = TwitterBuilder::new(db2.clone(), c_twitter)
        .screen_name(Account::from(config.twitter_screen_name))
        .consumer_key(config.twitter_api_key)
        .consumer_secret(config.twitter_api_secret)
        .sig_method("HMAC-SHA1".to_string())
        .token(config.twitter_token)
        .token_secret(config.twitter_token_secret)
        .version(1.0)
        .build()?;

    info!("Setting up Email client");
    let email = ClientBuilder::new(db2.clone(), c_email)
        .email_server(config.email_server)
        .imap_server(config.imap_server)
        .email_inbox(config.email_inbox)
        .email_user(config.email_user)
        .email_password(config.email_password)
        .build()?;

    info!("Starting Matrix task");
    tokio::spawn(async move {
        matrix.start().await;
    });

    info!("Starting Twitter task");
    tokio::spawn(async move {
        twitter.start().await;
    });

    info!("Starting Email task");
    tokio::spawn(async move {
        email.start().await;
    });

    if config.enable_watcher {
        info!("Starting Watcher connector task, listening...");
        let connector = connector.unwrap();

        tokio::spawn(async move {
            connector.start().await;
        });
    } else {
        warn!("Watcher connector task is disabled. Cannot process any requests...");
    }

    Ok(())
}
