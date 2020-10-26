use crate::primitives::Challenge;
use tokio::time::{self, Duration};

mod email_adapter;
mod matrix_adapter;
pub mod mocks;

// Generate a random db path
fn db_path() -> String {
    format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
}

async fn pause() {
    time::delay_for(Duration::from_secs(1)).await;
}
