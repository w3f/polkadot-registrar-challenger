use failure::Error;
use registrar::{block, run, init_env};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = init_env()?;

    run(config).await?;
    block().await;

    Ok(())
}
