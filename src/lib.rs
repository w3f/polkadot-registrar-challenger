#[macro_use]
extern crate tracing;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate async_trait;

use actix::clock::sleep;
use adapters::matrix::MatrixHandle;
use primitives::ChainName;
use std::fs;
use std::time::Duration;

pub type Result<T> = std::result::Result<T, anyhow::Error>;

use adapters::run_adapters;
use api::run_rest_api_server;
use connector::run_connector;
use database::Database;
use notifier::run_session_notifier;

mod adapters;
mod api;
mod connector;
mod database;
mod display_name;
mod notifier;
mod primitives;
#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub db: DatabaseConfig,
    pub instance: InstanceType,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "role", content = "config")]
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
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotifierConfig {
    pub api_address: String,
    pub display_name: DisplayNameConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterConfig {
    pub watcher: Vec<WatcherConfig>,
    pub matrix: MatrixConfig,
    pub twitter: TwitterConfig,
    pub email: EmailConfig,
    pub display_name: DisplayNameConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WatcherConfig {
    pub network: ChainName,
    pub endpoint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DisplayNameConfig {
    pub enabled: bool,
    pub limit: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MatrixConfig {
    pub enabled: bool,
    pub homeserver: String,
    pub username: String,
    pub password: String,
    pub db_path: String,
    pub admins: Option<Vec<MatrixHandle>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TwitterConfig {
    pub enabled: bool,
    pub api_key: String,
    pub api_secret: String,
    pub token: String,
    pub token_secret: String,
    pub request_interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
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
    let content = fs::read_to_string("config.yaml")
        .or_else(|_| fs::read_to_string("/etc/registrar/config.yaml"))
        .map_err(|_| {
            anyhow!("Failed to open config at 'config.yaml' or '/etc/registrar/config.yaml'.")
        })?;

    // Parse config file as JSON.
    let config = serde_yaml::from_str::<Config>(&content)
        .map_err(|err| anyhow!("Failed to parse config: {:?}", err))?;

    Ok(config)
}

async fn config_adapter_listener(db: Database, config: AdapterConfig) -> Result<()> {
    let watchers = config.watcher.clone();
    let dn_config = config.display_name.clone();
    run_adapters(config.clone(), db.clone()).await?;
    run_connector(db, watchers, dn_config).await
}

async fn config_session_notifier(
    db: Database,
    not_config: NotifierConfig,
) -> Result<()> {
    let lookup = run_rest_api_server(not_config, db.clone()).await?;

    actix::spawn(async move { run_session_notifier(db, lookup).await });

    Ok(())
}

// TODO: Check for database connectivity.
pub async fn run() -> Result<()> {
    let root = open_config()?;
    let (db_config, instance) = (root.db, root.instance);

    info!("Initializing connection to database");
    let db = Database::new(&db_config.uri, &db_config.name).await?;
    db.connectivity_check().await?;

    match instance {
        InstanceType::AdapterListener(config) => {
            info!("Starting adapter listener instance");
            config_adapter_listener(db, config).await?;
        }
        InstanceType::SessionNotifier(config) => {
            info!("Starting session notifier instance");
            config_session_notifier(db, config).await?;
        }
        InstanceType::SingleInstance(config) => {
            info!("Starting adapter listener and session notifier instances");
            let (adapter_config, notifier_config) = (config.adapter, config.notifier);

            config_adapter_listener(db.clone(), adapter_config).await?;
            config_session_notifier(db, notifier_config).await?;
        }
    }

    info!("Setup completed");

    loop {
        sleep(Duration::from_secs(u64::MAX)).await;
    }
}
