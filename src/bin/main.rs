#[macro_use]
extern crate tracing;

use system::{run, Result};

#[actix::main]
async fn main() -> Result<()> {
    run().await?;
    error!("Service exited unexpectedly");
    panic!();
}
