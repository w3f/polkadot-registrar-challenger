mod email;
mod matrix;
mod twitter;

pub use email::{SmtpImapClientBuilder, EmailHandler, EmailId};
pub use matrix::{MatrixClient, MatrixHandler};
pub use twitter::{Twitter, TwitterBuilder, TwitterId};
