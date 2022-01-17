#[macro_use]
extern crate log;
#[macro_use]
extern crate anyhow;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate async_trait;

use actix::clock::sleep;
use log::LevelFilter;
use primitives::ChainName;
use std::env;
use std::fs;
use std::time::Duration;

pub type Result<T> = std::result::Result<T, anyhow::Error>;

use actors::api::run_rest_api_server;
use actors::connector::run_connector;
use adapters::run_adapters;
use database::Database;
use notifier::SessionNotifier;

mod actors;
mod adapters;
mod database;
mod display_name;
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
#[serde(rename_all = "snake_case", tag = "a_type", content = "config")]
pub enum InstanceType {
    AdapterListener(AdapterConfig),
    SessionNotifier(NotifierConfig),
    SingleInstance(SingleInstanceConfig),
}

// TODO: Do all of those need to be pubic fields?
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

pub fn init_env() -> Result<Config> {
    let config = open_config()?;

    // Env variables for log level overwrites config.
    if let Ok(_) = env::var("RUST_LOG") {
        println!("Env variable 'RUST_LOG' found, overwriting logging level from config.");
        env_logger::init();
    } else {
        println!("Setting log level to '{}' from config.", config.log_level);
        env_logger::builder()
            .filter_module("system", config.log_level)
            .init();
    }

    println!("Logger initiated");

    Ok(config)
}

async fn config_adapter_listener(db_config: DatabaseConfig, config: AdapterConfig) -> Result<()> {
    let db = Database::new(&db_config.uri, &db_config.name).await?;

    // TODO: Pretty all the clones?
    let watchers = config.watcher.clone();
    run_adapters(config.clone(), db.clone()).await?;
    run_connector(db, watchers, config.display_name).await
}

async fn config_session_notifier(
    db_config: DatabaseConfig,
    not_config: NotifierConfig,
) -> Result<()> {
    let db = Database::new(&db_config.uri, &db_config.name).await?;
    let lookup = run_rest_api_server(not_config, db.clone()).await?;

    // TODO: Should be executed in `run_rest_api_server`
    actix::spawn(async move { SessionNotifier::new(db, lookup).run_blocking().await });

    Ok(())
}

// TODO: Check for database connectivity.
pub async fn run() -> Result<()> {
    let root = init_env()?;
    let (db_config, instance) = (root.db, root.instance);

    match instance {
        InstanceType::AdapterListener(config) => {
            info!("Starting adapter listener instance");
            config_adapter_listener(db_config, config).await?;
        }
        InstanceType::SessionNotifier(config) => {
            info!("Starting session notifier instance");
            config_session_notifier(db_config, config).await?;
        }
        InstanceType::SingleInstance(config) => {
            info!("Starting adapter listener and session notifier instances");
            let (adapter_config, notifier_config) = (config.adapter, config.notifier);

            config_adapter_listener(db_config.clone(), adapter_config).await?;
            config_session_notifier(db_config, notifier_config).await?;
        }
    }

    info!("Setup completed");

    loop {
        sleep(Duration::from_secs(u64::MAX)).await;
    }
}

#[actix::test]
async fn run_mocker() -> Result<()> {
    use crate::adapters::tests::MessageInjector;
    use crate::adapters::AdapterListener;
    use crate::database::Database;
    use crate::primitives::{
        ExpectedMessage, ExternalMessage, ExternalMessageType, JudgementState, MessageId, Timestamp,
    };
    use crate::tests::F;
    use rand::{thread_rng, Rng};

    // Init logger
    env_logger::builder()
        .filter_module("system", LevelFilter::Debug)
        .init();

    let mut rng = thread_rng();

    let db_config = DatabaseConfig {
        uri: "mongodb://localhost:27017".to_string(),
        name: format!("registrar_test_{}", rng.gen_range(u32::MIN..u32::MAX)),
    };

    let notifier_config = NotifierConfig {
        api_address: "localhost:8888".to_string(),
        display_name: DisplayNameConfig {
            enabled: true,
            limit: 0.85,
        },
    };

    info!("Starting mock adapter and session notifier instances");

    config_session_notifier(db_config.clone(), notifier_config).await?;

    // Setup database
    let db = Database::new(&db_config.uri, &db_config.name).await?;

    // Setup message verifier and injector.
    let injector = MessageInjector::new();
    let listener = AdapterListener::new(db.clone()).await;
    listener.start_message_adapter(injector.clone(), 1).await;

    info!("Mocker setup completed");

    let mut alice = JudgementState::alice();
    info!("INSERTING IDENTITY: Alice (1a2YiGNu1UUhJtihq8961c7FZtWGQuWDVMWTNBKJdmpGhZP)");
    db.add_judgement_request(&alice).await.unwrap();

    // Create messages and (valid/invalid) messages randomly.
    let mut rng = thread_rng();
    loop {
        let ty_msg: u32 = rng.gen_range(0..2);
        let ty_validity = rng.gen_range(0..1);
        let reset = rng.gen_range(0..5);

        match reset {
            // Reset state.
            0 => {
                warn!("Resetting Identity");
                db.delete_judgement(&alice.context).await.unwrap();
                alice = JudgementState::alice();
                db.add_judgement_request(&alice).await.unwrap();
            }
            _ => {}
        }

        let (origin, values) = match ty_msg {
            0 => {
                (ExternalMessageType::Email("alice@email.com".to_string()), {
                    // Get either valid or invalid message
                    match ty_validity {
                        0 => alice
                            .get_field(&F::ALICE_EMAIL())
                            .expected_message()
                            .to_message_parts(),
                        1 => ExpectedMessage::random().to_message_parts(),
                        _ => panic!(),
                    }
                })
            }
            1 => {
                (ExternalMessageType::Twitter("@alice".to_string()), {
                    // Get either valid or invalid message
                    match ty_validity {
                        0 => alice
                            .get_field(&F::ALICE_TWITTER())
                            .expected_message()
                            .to_message_parts(),
                        1 => ExpectedMessage::random().to_message_parts(),
                        _ => panic!(),
                    }
                })
            }
            2 => {
                (
                    ExternalMessageType::Matrix("@alice:matrix.org".to_string()),
                    {
                        // Get either valid or invalid message
                        match ty_validity {
                            0 => alice
                                .get_field(&F::ALICE_MATRIX())
                                .expected_message()
                                .to_message_parts(),
                            1 => ExpectedMessage::random().to_message_parts(),
                            _ => panic!(),
                        }
                    },
                )
            }
            _ => panic!(),
        };

        // Inject message.
        injector
            .send(ExternalMessage {
                origin: origin,
                id: MessageId::from(0u32),
                timestamp: Timestamp::now(),
                values: values,
            })
            .await;

        sleep(Duration::from_secs(3)).await;
    }
}
