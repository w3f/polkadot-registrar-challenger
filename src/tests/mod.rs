use crate::primitives::Challenge;
use tokio::time::{self, Duration};

pub mod mocks;
mod matrix_adapter;

// Generate a random db path
fn db_path() -> String {
    format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
}

async fn pause() {
    time::delay_for(Duration::from_secs(1)).await;
}
