#[macro_use]
extern crate log;

use failure::Error;
use registrar::{block, init_env, run};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = init_env()?;

    run(config)
        .await
        .map_err(|err| {
            error!("{}", err);
            std::process::exit(1);
        })
        .unwrap();

    block().await;

    Ok(())
}
