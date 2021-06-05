#[macro_use]
extern crate log;
#[macro_use]
extern crate thiserror;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate async_trait;

use futures::{select, FutureExt};
use log::LevelFilter;
use std::env;
use std::fs;

pub type Result<T> = std::result::Result<T, anyhow::Error>;

// Re-exports
pub use actors::api::run_rest_api_server_blocking;
pub use adapters::run_adapters_blocking;
pub use database::Database;
pub use notifier::SessionNotifier;

mod actors;
mod adapters;
mod database;
mod notifier;
mod primitives;
#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub log_level: LevelFilter,
    pub db: DatabaseConfig,
    pub instance: InstanceType,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "config")]
pub enum InstanceType {
    AdapterListener(AdapterConfig),
    SessionNotifier(NotifierConfig),
    SingleInstance(SingleInstanceConfig),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SingleInstanceConfig {
    pub adapter: AdapterConfig,
    pub notifier: NotifierConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseConfig {
    pub uri: String,
    pub db_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotifierConfig {
    pub api_address: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterConfig {
    pub accounts: AccountsConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
// TODO: Rename to "Adapter"?
pub struct AccountsConfig {
    matrix: MatrixConfig,
    twitter: TwitterConfig,
    email: EmailConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MatrixConfig {
    pub enabled: bool,
    pub homeserver: String,
    pub username: String,
    pub password: String,
    pub db_path: String,
    // Since the Matrix SDK listens to responses in a stream, this value does
    // not require special considerations. But it should be often enough, given
    // that `AdapterListener` fetches the messages from the queue in intervals.
    pub request_interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TwitterConfig {
    pub enabled: bool,
    pub api_key: String,
    pub api_secret: String,
    pub token: String,
    pub token_secret: String,
    pub request_interval: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EmailConfig {
    pub enabled: bool,
    pub smtp_server: String,
    pub imap_server: String,
    pub inbox: String,
    pub user: String,
    pub password: String,
    pub request_interval: u64,
}

fn open_config() -> Result<Config> {
    // Open config file.
    let content = fs::read_to_string("config.json")
        .or_else(|_| fs::read_to_string("/etc/registrar/config.json"))
        .map_err(|_| {
            eprintln!("Failed to open config at 'config.json' or '/etc/registrar/config.json'.");
            std::process::exit(1);
        })
        .unwrap();

    // Parse config file as JSON.
    let config = serde_yaml::from_str::<Config>(&content)
        .map_err(|err| {
            eprintln!("Failed to parse config: {}", err);
            std::process::exit(1);
        })
        .unwrap();

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

async fn config_adapter_listener(db_config: DatabaseConfig, config: AdapterConfig) -> Result<()> {
    let db = Database::new(&db_config.uri, &db_config.db_name).await?;
    run_adapters_blocking(config.accounts, db).await
}

async fn config_session_notifier(db_config: DatabaseConfig, config: NotifierConfig) -> Result<()> {
    let db = Database::new(&db_config.uri, &db_config.db_name).await?;
    let server = run_rest_api_server_blocking(&config.api_address, db.clone()).await?;
    Ok(SessionNotifier::new(db, server).run_blocking().await)
}

pub async fn run() -> Result<()> {
    let root = init_env()?;
    let (db_config, instance) = (root.db, root.instance);

    match instance {
        InstanceType::AdapterListener(config) => {
            config_adapter_listener(db_config, config).await?;
        }
        InstanceType::SessionNotifier(config) => {
            config_session_notifier(db_config, config).await?;
        }
        InstanceType::SingleInstance(config) => {
            let (adapter, notifier) = (config.adapter, config.notifier);

            let t1_db_config = db_config.clone();
            let t2_db_config = db_config.clone();

            // Starts threads
            let a =
                tokio::spawn(async move { config_adapter_listener(t1_db_config, adapter).await });
            let b =
                tokio::spawn(async move { config_session_notifier(t2_db_config, notifier).await });

            // If one thread exits, exit the full application.
            select! {
                res = a.fuse() => res??,
                res = b.fuse() => res??,
            }
        }
    }

    Ok(())
}

#[actix::test]
async fn random_generator() -> Result<()> {
    let root = init_env()?;
    let (db_config, instance) = (root.db, root.instance);

    match instance {
        InstanceType::SingleInstance(config) => {
            let (adapter, notifier) = (config.adapter, config.notifier);

            let t1_db_config = db_config.clone();
            let t2_db_config = db_config.clone();

            let a =
                tokio::spawn(async move { config_adapter_listener(t1_db_config, adapter).await });
            let b =
                tokio::spawn(async move { config_session_notifier(t2_db_config, notifier).await });

            // If one thread exits, exit the full application.
            select! {
                res = a.fuse() => res??,
                res = b.fuse() => res??,
            }
        }
        _ => panic!("wrong instance type in config"),
    }

    Ok(())
}
