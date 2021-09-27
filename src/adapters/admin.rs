use crate::primitives::ChainAddress;
use std::str::FromStr;

pub type Result<T> = std::result::Result<T, Response>;

pub enum Command {
    Status(ChainAddress),
    Verify(ChainAddress, Vec<FieldName>),
}

impl FromStr for Command {
    type Err = Response;

    fn from_str(s: &str) -> Result<Self> {
        // Convenience handler.
        let s = s.trim().replace("  ", " ");

        if s.starts_with("status") {
            let parts: Vec<&str> = s.split(" ").skip(1).collect();
            if parts.len() != 1 {
                return Err(Response::InvalidSyntax(None));
            }

            Ok(Command::Status(ChainAddress::from(parts[0].to_string())))
        } else if s.starts_with("verify") {
            let parts: Vec<&str> = s.split(" ").skip(1).collect();
            if parts.len() < 2 {
                return Err(Response::InvalidSyntax(None));
            }

            Ok(Command::Verify(
                ChainAddress::from(parts[0].to_string()),
                parts[1..]
                    .into_iter()
                    .map(|s| FieldName::from_str(s))
                    .collect::<Result<Vec<FieldName>>>()?,
            ))
        } else {
            Err(Response::UnknownCommand)
        }
    }
}

pub enum Response {
    Status(ChainAddress),
    Verified(ChainAddress, Vec<FieldName>),
    UnknownCommand,
    InvalidSyntax(Option<String>),
    InternalError,
	Help,
}

// Raw field name
pub enum FieldName {
    LegalName,
    DisplayName,
    Email,
    Web,
    Twitter,
    Matrix,
}

impl FromStr for FieldName {
    type Err = Response;

    fn from_str(s: &str) -> Result<Self> {
        // Convenience handler.
        let s = s.trim().replace("-", "").replace("_", "").to_lowercase();

        let f = match s.as_str() {
            "legalname" => FieldName::LegalName,
            "displayname" => FieldName::DisplayName,
            "email" => FieldName::Email,
            "web" => FieldName::Web,
            "twitter" => FieldName::Twitter,
            "matrix" => FieldName::Matrix,
            _ => return Err(Response::InvalidSyntax(Some(s.to_string()))),
        };

        Ok(f)
    }
}

pub fn process_admin() -> Result<()> {
    unimplemented!()
}
