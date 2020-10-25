use crate::primitives::Challenge;
use tokio::time::{self, Duration};

pub mod mocks;
mod verify_matrix;

// Generate a random db path
fn db_path() -> String {
    format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
}

async fn pause() {
    time::delay_for(Duration::from_millis(100)).await;
}
