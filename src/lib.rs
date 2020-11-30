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
use comms::{CommsMain, CommsVerifier};
use connector::{Connector, ConnectorInitTransports};
pub use connector::{
    ConnectorReaderTransport, ConnectorWriterTransport, WebSocketReader, WebSocketWriter,
    WebSockets,
};
pub use db::Database;
pub use health_check::HealthCheck;
use manager::{IdentityManager, IdentityManagerConfig};
pub use primitives::Account;
use primitives::{AccountType, Fatal, Result};
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::process::exit;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tests::mocks::{ConnectorMocker, ConnectorReaderMocker, EventManager};
use tokio::time::{self, Duration};

pub mod adapters;
mod comms;
mod connector;
mod db;
mod health_check;
mod manager;
mod primitives;
#[cfg(test)]
mod tests;
mod verifier;

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
    db2: Database,
    matrix_transport: M,
    twitter_transport: T,
    email_transport: E,
) -> Result<()> {
    let (_, c_connector) = run_adapters(
        db2.clone(),
        Default::default(),
        matrix_transport,
        twitter_transport,
        email_transport,
    )
    .await?;

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

#[cfg(test)]
pub async fn test_run<
    M: MatrixTransport,
    T: Clone + TwitterTransport,
    E: Clone + EmailTransport,
>(
    event_manager: Arc<EventManager>,
    db2: Database,
    identity_manager_config: IdentityManagerConfig,
    matrix_transport: M,
    twitter_transport: T,
    email_transport: E,
) -> Result<TestRunReturn> {
    let (c_matrix, c_connector) = run_adapters(
        db2.clone(),
        identity_manager_config,
        matrix_transport,
        twitter_transport,
        email_transport,
    )
    .await?;

    let mut connector = Connector::new::<ConnectorMocker>(event_manager.clone(), c_connector)
        .await
        .unwrap();

    let (writer, reader) = ConnectorMocker::init(event_manager).await.unwrap();
    connector.set_writer_reader(writer.clone(), reader.clone());

    tokio::spawn(async move {
        connector.start::<ConnectorMocker>().await;
    });

    time::delay_for(Duration::from_secs(1)).await;

    Ok(TestRunReturn {
        matrix: c_matrix,
        reader: reader,
    })
}

#[cfg(test)]
pub struct TestRunReturn {
    matrix: CommsMain,
    reader: ConnectorReaderMocker,
}

async fn run_adapters<
    M: MatrixTransport,
    T: Clone + TwitterTransport,
    E: Clone + EmailTransport,
>(
    db2: Database,
    identity_manager_config: IdentityManagerConfig,
    mut matrix_transport: M,
    twitter_transport: T,
    email_transport: E,
) -> Result<(CommsMain, CommsVerifier)> {
    info!("Setting up manager");
    let mut manager = IdentityManager::new(db2.clone(), identity_manager_config)?;

    info!("Setting up communication channels");
    let c_connector = manager.register_comms(AccountType::ReservedConnector);
    let c_emitter = manager.register_comms(AccountType::ReservedEmitter);
    let c_display_name = manager.register_comms(AccountType::DisplayName);
    let c_matrix = manager.register_comms(AccountType::Matrix);
    let c_twitter = manager.register_comms(AccountType::Twitter);
    let c_email = manager.register_comms(AccountType::Email);

    // Since the Matrix event emitter runs in the background, the handling of
    // messages must be tested by using this `CommsVerifier` handle and sending
    // `TriggerMatrixEmitter` messages.
    let main_matrix = manager.get_comms(&AccountType::Matrix)?.clone();

    info!("Starting manager task");
    tokio::spawn(async move {
        manager.start().await;
    });

    info!("Starting display name handler");
    let l_db = db2.clone();
    tokio::spawn(async move {
        DisplayNameHandler::new(l_db, c_display_name, 0.85)
            .start()
            .await;
    });

    info!("Starting Matrix task");
    let l_db = db2.clone();
    let l_c_matrix = c_matrix.clone();
    tokio::spawn(async move {
        matrix_transport.run_emitter(l_db.clone(), c_emitter).await;

        MatrixHandler::new(l_db, l_c_matrix, matrix_transport)
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

    Ok((main_matrix, c_connector))
}
