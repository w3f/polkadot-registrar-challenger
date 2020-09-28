pub mod email;
pub mod matrix;
pub mod twitter;

pub use email::{ClientBuilder, EmailId};
pub use matrix::MatrixClient;
pub use twitter::{Twitter, TwitterBuilder, TwitterId};
