#[macro_use]
extern crate log;
#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate failure;

use adapters::{
    DisplayNameHandler, EmailHandler, EmailTransport, MatrixHandler, MatrixTransport,
    TwitterHandler, TwitterTransport,
};
pub use adapters::{MatrixClient, SmtpImapClientBuilder, TwitterBuilder};
use connector::{Connector, ConnectorInitTransports};
pub use connector::{
    ConnectorReaderTransport, ConnectorWriterTransport, WebSocketReader, WebSocketWriter,
    WebSockets,
};
pub use db::Database2;
pub use health_check::HealthCheck;
use manager::IdentityManager;
pub use primitives::Account;
use primitives::{AccountType, Fatal, Result};
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::process::exit;
use tokio::time::{self, Duration};

pub mod adapters;
mod comms;
mod connector;
mod db;
mod health_check;
mod manager;
mod primitives;
mod verifier;
mod tests;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub registrar_db_path: String,
    pub matrix_db_path: String,
    pub log_level: log::LevelFilter,
    pub watcher_url: String,
    pub enable_watcher: bool,
    pub enable_accounts: bool,
    pub enable_health_check: bool,
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

pub async fn run<
    C: ConnectorInitTransports<W, R, Endpoint = P>,
    W: 'static + Send + Sync + ConnectorWriterTransport,
    R: 'static + Send + Sync + ConnectorReaderTransport,
    P: 'static + Send + Sync + Clone,
    M: MatrixTransport,
    T: Clone + TwitterTransport,
    E: Clone + EmailTransport,
>(
    enable_watcher: bool,
    watcher_url: P,
    db2: Database2,
    mut matrix_transport: M,
    twitter_transport: T,
    email_transport: E,
) -> Result<()> {
    info!("Setting up manager");
    let mut manager = IdentityManager::new(db2.clone())?;

    info!("Setting up communication channels");
    let c_connector = manager.register_comms(AccountType::ReservedConnector);
    let c_emitter = manager.register_comms(AccountType::ReservedEmitter);
    let c_display_name = manager.register_comms(AccountType::DisplayName);
    let c_matrix = manager.register_comms(AccountType::Matrix);
    let c_twitter = manager.register_comms(AccountType::Twitter);
    let c_email = manager.register_comms(AccountType::Email);

    info!("Starting manager task");
    tokio::spawn(async move {
        manager.start().await;
    });

    info!("Starting display name handler");
    let l_db = db2.clone();
    tokio::spawn(async move {
        DisplayNameHandler::new(l_db, c_display_name, 0.8)
            .start()
            .await;
    });

    info!("Starting Matrix task");
    let l_db = db2.clone();
    tokio::spawn(async move {
        matrix_transport.run_emitter(l_db.clone(), c_emitter).await;

        MatrixHandler::new(l_db, c_matrix, matrix_transport)
            .start()
            .await;
    });

    info!("Starting Twitter task");
    let l_db = db2.clone();
    tokio::spawn(async move {
        TwitterHandler::new(l_db, c_twitter)
            .start(twitter_transport)
            .await;
    });

    info!("Starting Email task");
    let l_db = db2.clone();
    tokio::spawn(async move {
        EmailHandler::new(l_db, c_email)
            .start(email_transport)
            .await;
    });

    if enable_watcher {
        info!("Trying to connect to Watcher");
        let mut counter = 0;
        let mut interval = time::interval(Duration::from_secs(5));

        let connector;
        loop {
            interval.tick().await;

            if let Ok(con) = Connector::new::<C>(watcher_url.clone(), c_connector.clone()).await {
                info!("Connecting to Watcher succeeded");
                connector = con;
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

        info!("Starting Watcher connector task, listening...");
        tokio::spawn(async move {
            connector.start::<C>().await;
        });
    } else {
        warn!("Watcher connector task is disabled. Cannot process any requests...");
    }

    Ok(())
}
