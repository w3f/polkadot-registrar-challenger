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

use manager::NetworkAddress;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;

pub type Result<T> = std::result::Result<T, anyhow::Error>;

mod adapters;
mod aggregate;
mod api;
mod event;
mod manager;
mod projection;
mod system;

#[derive(Debug, Error)]
pub enum Error {
    #[error("The target address was not found ({}): {:?}", _1, _0)]
    TargetAddressNotFound(NetworkAddress, String),
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub accounts: AccountsConfig,
    pub log_level: log::LevelFilter,
}

#[derive(Debug, Deserialize)]
pub struct AccountsConfig {
    matrix: MatrixConfig,
    twitter: TwitterConfig,
    email: EmailConfig,
}

#[derive(Debug, Deserialize)]
pub struct MatrixConfig {
    pub enabled: bool,
    pub homeserver: String,
    pub username: String,
    pub password: String,
    pub db_path: String,
}

#[derive(Debug, Deserialize)]
pub struct TwitterConfig {
    pub enabled: bool,
    pub screen_name: String,
    pub api_key: String,
    pub api_secret: String,
    pub token: String,
    pub token_secret: String,
}

#[derive(Debug, Deserialize)]
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
