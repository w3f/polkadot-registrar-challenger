use crate::actors::connector::create_context;
use crate::primitives::{ChainAddress, JudgementState, JudgementStateBlanked};
use crate::Database;
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
    Status(JudgementStateBlanked),
    Verified(ChainAddress, Vec<FieldName>),
    UnknownCommand,
    IdentityNotFound,
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

impl std::fmt::Display for FieldName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", {
            match self {
                FieldName::LegalName => "legal_name",
                FieldName::DisplayName => "display_name",
                FieldName::Email => "email",
                FieldName::Web => "web",
                FieldName::Twitter => "twitter",
                FieldName::Matrix => "matrix",
            }
        })
    }
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

pub async fn process_admin(db: Database, command: Command) -> Response {
    let local = |db: Database, command: Command| async move {
        match command {
            Command::Status(addr) => {
                let context = create_context(addr);
                let state = db.fetch_judgement_state(&context).await?;

                // Determine response based on database lookup.
                match state {
                    Some(state) => Ok(Response::Status(state.into())),
                    None => Ok(Response::IdentityNotFound),
                }
            }
            Command::Verify(addr, fields) => {
                let context = create_context(addr.clone());

                // Verify each passed on field.
                for field in &fields {
                    if db.verify_manually(&context, field).await?.is_none() {
                        return Ok(Response::IdentityNotFound);
                    }
                }

                Ok(Response::Verified(addr, fields))
            }
        }
    };

    let res: crate::Result<Response> = local(db, command).await;
    match res {
        Ok(_) => {}
        Err(_) => {}
    }

    unimplemented!()
}
