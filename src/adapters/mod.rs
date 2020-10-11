mod email;
mod matrix;
mod twitter;

pub use email::{SmtpImapClientBuilder, EmailHandler, EmailId};
pub use matrix::MatrixClient;
pub use twitter::{Twitter, TwitterBuilder, TwitterId};
