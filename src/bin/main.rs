use failure::Error;
use registrar::{block, init_env, run};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let config = init_env()?;

    run(config).await?;
    block().await;

    Ok(())
}
