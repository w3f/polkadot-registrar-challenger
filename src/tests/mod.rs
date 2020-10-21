use crate::primitives::Challenge;

pub mod mocks;
mod verify_matrix;

// Generate a random db path
fn db_path() -> String {
    format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
}