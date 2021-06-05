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

use log::LevelFilter;
use std::env;
use std::fs;

pub type Result<T> = std::result::Result<T, anyhow::Error>;

mod actors;
mod adapters;
mod database;
mod notifier;
mod primitives;
#[cfg(test)]
mod tests;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum Config {
    Adapter(AdapterConfig),
    Notifier(NotifierConfig),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DatabaseConfig {
    pub uri: String,
    pub db_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NotifierConfig {
    pub db: DatabaseConfig,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterConfig {
    pub accounts: AccountsConfig,
    pub db: DatabaseConfig,
    pub log_level: LevelFilter,
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

pub fn init_env(level: LevelFilter) -> Result<Config> {
    let config = open_config()?;

    // Env variables for log level overwrites config.
    if let Ok(_) = env::var("RUST_LOG") {
        println!("Env variable 'RUST_LOG' found, overwriting logging level from config.");
        env_logger::init();
    } else {
        println!("Setting log level to '{}' from config.", level);
        env_logger::builder()
            .filter_module("registrar", level)
            .init();
    }

    println!("Logger initiated");

    Ok(config)
}
