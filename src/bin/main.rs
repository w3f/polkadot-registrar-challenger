#[macro_use]
extern crate tracing;

use system::{run, Result};
use tracing::Level;

#[actix::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        //.with_env_filter("system")
        .init();

    run().await?;
    unreachable!()
}
