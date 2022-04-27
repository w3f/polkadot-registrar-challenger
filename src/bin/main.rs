#[macro_use]
extern crate tracing;

use system::{run, Result};

#[actix::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    run().await?;
    error!("Service exited unexpectedly");
    panic!("Service exited unexpectedly");
}
