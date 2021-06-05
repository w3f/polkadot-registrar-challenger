use libregistrar::{
    init_env, run_rest_api_server_blocking, Config, Database, Result, SessionNotifier,
};

#[actix::main]
async fn main() -> Result<()> {
    let config = init_env()?;

    match config {
        Config::AdapterListener(config) => {}
        Config::SessionNotifier(config) => {
            let db = Database::new(&config.db.uri, &config.db.db_name).await?;
            let server = run_rest_api_server_blocking(&config.api_address, db.clone()).await?;
            SessionNotifier::new(db, server);
        }
    }

    Ok(())
}
