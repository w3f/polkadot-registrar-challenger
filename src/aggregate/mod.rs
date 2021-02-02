pub mod display_name;
mod message_watcher;
pub mod verifier;

// Expose publicly
pub use message_watcher::{MessageWatcher, MessageWatcherCommand, MessageWatcherId};

/// This wrapper type is required for the use of `eventually`, since
/// `anyhow::Error` does not implement `std::error::Error`.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct Error(#[from] anyhow::Error);
