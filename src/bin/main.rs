use failure::Error;
use lib::{block, run, Config};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let config = Config {
        registrar_db_path: "/tmp/registrar.db".to_string(),
        matrix_db_path: "/tmp/matrix.db".to_string(),
        //watcher_url: "ws://test-registrar-watcher:3001".to_string(),
        watcher_url: "ws://localhost:3001".to_string(),
        enable_watcher: true,
        matrix_homeserver: env::var("TEST_MATRIX_HOMESERVER").unwrap(),
        matrix_username: env::var("TEST_MATRIX_USER").unwrap(),
        matrix_password: env::var("TEST_MATRIX_PASSWORD").unwrap(),
    };

    run(config).await?;
    block().await;

    Ok(())
}
