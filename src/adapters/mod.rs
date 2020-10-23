mod display_name;
pub(crate) mod email;
mod matrix;
pub(crate) mod twitter;

pub use display_name::{DisplayNameHandler, VIOLATIONS_CAP};
pub use email::{EmailHandler, EmailId, EmailTransport, SmtpImapClientBuilder};
pub use matrix::{EventExtract, MatrixClient, MatrixHandler, MatrixTransport};
pub use twitter::{Twitter, TwitterBuilder, TwitterHandler, TwitterId, TwitterTransport};
