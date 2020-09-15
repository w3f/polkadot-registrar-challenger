use lib::{run, Config};
use std::env;

#[tokio::main]
async fn main() {
    let config = Config {
        db_path: "/tmp/matrix_db".to_string(),
        matrix_homeserver: env::var("TEST_MATRIX_HOMESERVER").unwrap(),
        matrix_username: env::var("TEST_MATRIX_USER").unwrap(),
        matrix_password: env::var("TEST_MATRIX_PASSWORD").unwrap(),
    };

    run(config).await;
}
