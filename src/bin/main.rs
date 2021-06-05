#[macro_use]
extern crate log;
#[macro_use]
extern crate anyhow;

use futures::{select, FutureExt};
use libregistrar::{
    init_env, run_adapters_blocking, run_rest_api_server_blocking, AdapterConfig, Database,
    InstanceType, NotifierConfig, Result, SessionNotifier,
};

async fn config_adapter_listener(config: AdapterConfig) -> Result<()> {
    let db = Database::new(&config.db.uri, &config.db.db_name).await?;
    run_adapters_blocking(config.accounts, db).await
}

async fn config_session_notifier(config: NotifierConfig) -> Result<()> {
    let db = Database::new(&config.db.uri, &config.db.db_name).await?;
    let server = run_rest_api_server_blocking(&config.api_address, db.clone()).await?;
    Ok(SessionNotifier::new(db, server).run_blocking().await)
}

#[actix::main]
async fn main() -> Result<()> {
    let config = init_env()?;

    match config.instance {
        InstanceType::AdapterListener(config) => {
            config_adapter_listener(config).await?;
        }
        InstanceType::SessionNotifier(config) => {
            config_session_notifier(config).await?;
        }
        InstanceType::SingleInstance(config) => {
            let (adapter, notifier) = (config.adapter, config.notifier);

            let a = tokio::spawn(async move { config_adapter_listener(adapter).await });
            let b = tokio::spawn(async move { config_session_notifier(notifier).await });

            // If one thread exits, exit the full application.
            select! {
                res = a.fuse() => res??,
                res = b.fuse() => res??,
            }
        }
    }

    error!("Service exited unexpectedly");

    Ok(())
}
