#[macro_use]
extern crate log;
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
pub use actors::api::run_rest_api_server;
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
// TODO: Rename to "Adapter"?
pub struct AdapterConfig {
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
    run_adapters_blocking(config, db).await
}

async fn config_session_notifier(db_config: DatabaseConfig, config: NotifierConfig) -> Result<()> {
    let db = Database::new(&db_config.uri, &db_config.db_name).await?;
    let lookup = run_rest_api_server(&config.api_address, db.clone()).await?;

    actix::spawn(async move { SessionNotifier::new(db, lookup).run_blocking().await });

    Ok(())
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
            let a = actix::spawn(async move {
                config_adapter_listener(t1_db_config, adapter)
                    .await
                    .map(|_| ())
                    .or_else::<(), _>(|err| {
                        error!("{:?}", err);
                        Ok(())
                    })
                    .unwrap()
            });
            // TOOD: No need to run in thread. See blow in test function.
            let b = actix::spawn(async move {
                config_session_notifier(t2_db_config, notifier)
                    .await
                    .map(|_| ())
                    .or_else::<(), _>(|err| {
                        error!("{:?}", err);
                        Ok(())
                    })
                    .unwrap()
            });

            // If one thread exits, exit the full application.
            select! {
                res = a.fuse() => res?,
                res = b.fuse() => res?,
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::primitives::{
        ExpectedMessage, ExternalMessage, ExternalMessageType, JudgementState, MessageId, Timestamp,
    };
    use rand::{thread_rng, Rng};
    use tokio::time::{sleep, Duration};

    #[actix::test]
    #[ignore]
    // TODO: Implement custom Ctrl+C shutdown logic: https://stackoverflow.com/questions/54569843/shutting-down-actix-with-more-than-one-system-running
    // Client message: `{"chain":"polkadot","address":"1a2YiGNu1UUhJtihq8961c7FZtWGQuWDVMWTNBKJdmpGhZP"}`
    async fn random_event_generator() {
        env_logger::builder()
            .filter_level(LevelFilter::Debug)
            .init();

        async fn generate_events(db_config: DatabaseConfig) {
            let mut db = Database::new(&db_config.uri, &db_config.db_name)
                .await
                .unwrap();

            let mut alice = JudgementState::alice();
            let bob = JudgementState::bob();

            loop {
                let rand = thread_rng().gen_range(0, 3);
                match rand {
                    0 => alice = JudgementState::alice_unsupported(),
                    _ => {}
                }

                db.add_judgement_request(alice.clone()).await.unwrap();
                db.add_judgement_request(bob.clone()).await.unwrap();

                let rand = thread_rng().gen_range(0, 3);
                match rand {
                    0 => db.set_display_name_valid("Alice").await.unwrap(),
                    _ => {}
                }

                let rand = thread_rng().gen_range(0, 3);
                let origin = match rand {
                    0 => ExternalMessageType::Email("alice@email.com".to_string()),
                    1 => ExternalMessageType::Matrix("@alice:matrix.org".to_string()),
                    2 => ExternalMessageType::Twitter("@alice".to_string()),
                    _ => panic!(),
                };

                let msg = ExternalMessage {
                    origin: origin.clone(),
                    id: MessageId::from(0u32),
                    timestamp: Timestamp::now(),
                    values: {
                        let rand = thread_rng().gen_range(0, 2);
                        match rand {
                            0 => ExpectedMessage::random().to_message_parts(),
                            1 => alice
                                .get_field(&origin.into())
                                .expected_message()
                                .to_message_parts(),
                            _ => panic!(),
                        }
                    },
                };

                db.process_message(&msg).await.unwrap();

                let rand = thread_rng().gen_range(3, 5);
                sleep(Duration::from_secs(rand)).await;
                db.prune_timeout().await.unwrap();
                sleep(Duration::from_secs(rand)).await;
            }
        }

        let database = DatabaseConfig {
            uri: "mongodb://localhost:27017/".to_string(),
            db_name: format!(
                "random_event_generator_{}",
                thread_rng().gen_range(u32::MIN, u32::MAX)
            ),
        };

        let notifier = NotifierConfig {
            api_address: "localhost:8888".to_string(),
        };

        let t_db = database.clone();

        config_session_notifier(database, notifier).await.unwrap();
        actix::spawn(async move { generate_events(t_db).await })
            .await
            .unwrap();
    }
}
