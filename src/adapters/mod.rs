mod display_name;
mod email;
mod matrix;
mod twitter;

pub use display_name::StringMatcher;
pub use email::{EmailHandler, EmailId, EmailTransport, SmtpImapClientBuilder};
pub use matrix::{MatrixClient, MatrixHandler, MatrixTransport};
pub use twitter::{Twitter, TwitterBuilder, TwitterHandler, TwitterId, TwitterTransport};
