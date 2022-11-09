use system::{run, Result};

#[actix::main]
async fn main() -> Result<()> {
    run().await?;
    unreachable!()
}
