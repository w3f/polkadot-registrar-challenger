#[macro_use]
extern crate log;

use failure::Error;
use registrar::{block, init_env, run};
use registrar::{
    Account, Database2, HealthCheck, MatrixClient, SmtpImapClientBuilder, TwitterBuilder,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = init_env()?;

    info!("Setting up database");
    let db2 = Database2::new(&config.registrar_db_path)?;

    info!("Starting health check thread");
    if config.enable_health_check {
        std::thread::spawn(|| {
            HealthCheck::start()
                .map_err(|err| {
                    error!("Failed to start health check service: {}", err);
                    std::process::exit(1);
                })
                .unwrap();
        });
    }

    if config.enable_accounts {
        info!("Setting up Matrix client");
        let matrix_transport = MatrixClient::new(
            &config.matrix_homeserver,
            &config.matrix_username,
            &config.matrix_password,
            &config.matrix_db_path,
            db2.clone(),
        )
        .await?;

        info!("Setting up Twitter client");
        let twitter_transport = TwitterBuilder::new()
            .screen_name(Account::from(config.twitter_screen_name))
            .consumer_key(config.twitter_api_key)
            .consumer_secret(config.twitter_api_secret)
            .sig_method("HMAC-SHA1".to_string())
            .token(config.twitter_token)
            .token_secret(config.twitter_token_secret)
            .version(1.0)
            .build()?;

        info!("Setting up Email client");
        let email_transport = SmtpImapClientBuilder::new()
            .email_server(config.email_server)
            .imap_server(config.imap_server)
            .email_inbox(config.email_inbox)
            .email_user(config.email_user)
            .email_password(config.email_password)
            .build()?;

        run(
            config.enable_watcher,
            &config.watcher_url,
            db2,
            matrix_transport,
            twitter_transport,
            email_transport,
        )
        .await
        .map_err(|err| {
            error!("{}", err);
            std::process::exit(1);
        })
        .unwrap();
    }

    block().await;

    Ok(())
}
