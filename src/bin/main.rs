#[macro_use]
extern crate log;

use system::{run, Result};

#[actix::main]
async fn main() -> Result<()> {
    run().await?;
    error!("Service exited unexpectedly");
    Ok(())
}
