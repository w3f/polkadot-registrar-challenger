use crate::actors::connector::create_context;
use crate::primitives::{ChainAddress, JudgementState, JudgementStateBlanked};
use crate::Database;
use std::str::FromStr;

pub type Result<T> = std::result::Result<T, Response>;

pub enum Command {
    Status(ChainAddress),
    Verify(ChainAddress, Vec<RawFieldName>),
}

impl FromStr for Command {
    type Err = Response;

    fn from_str(s: &str) -> Result<Self> {
        // Convenience handler.
        let s = s.trim().replace("  ", " ");

        if s.starts_with("status") {
            let parts: Vec<&str> = s.split(" ").skip(1).collect();
            if parts.len() != 1 {
                return Err(Response::UnknownCommand);
            }

            Ok(Command::Status(ChainAddress::from(parts[0].to_string())))
        } else if s.starts_with("verify") {
            let parts: Vec<&str> = s.split(" ").skip(1).collect();
            if parts.len() < 2 {
                return Err(Response::UnknownCommand);
            }

            Ok(Command::Verify(
                ChainAddress::from(parts[0].to_string()),
                parts[1..]
                    .into_iter()
                    .map(|s| RawFieldName::from_str(s))
                    .collect::<Result<Vec<RawFieldName>>>()?,
            ))
        } else {
            Err(Response::UnknownCommand)
        }
    }
}

pub enum Response {
    Status(JudgementStateBlanked),
    Verified(ChainAddress, Vec<RawFieldName>),
    UnknownCommand,
    IdentityNotFound,
    InvalidSyntax(Option<String>),
    Help,
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Response::Status(state) => {
                format!("")
            }
            Response::Verified(_, fields) => {
                format!("Verified the following fields: {}", {
                    let mut all = String::new();
                    for field in fields {
                        all.push_str(&format!("{}, ", field.to_string()));
                    }

                    // Remove `, ` suffix.
                    all.pop();
                    all.pop();

                    all
                })
            }
            Response::UnknownCommand => panic!(),
            Response::IdentityNotFound => {
                format!("There is no pending judgement request for the provided identity")
            }
            Response::InvalidSyntax(input) => {
                format!(
                    "Invalid input{}",
                    match input {
                        Some(input) => format!(" '{}'", input),
                        None => format!(""),
                    }
                )
            }
            Response::Help => {
                format!(
                    "\
                status <ADDR>\tShow the current verification status of the specified address.\n\
                verify <ADDR> <FIELD>...\tVerify one or multiple fields of the specified address.\n\
                "
                )
            }
        };

        write!(f, "{}", msg)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum RawFieldName {
    LegalName,
    DisplayName,
    Email,
    Web,
    Twitter,
    Matrix,
}

impl std::fmt::Display for RawFieldName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", {
            match self {
                RawFieldName::LegalName => "legal_name",
                RawFieldName::DisplayName => "display_name",
                RawFieldName::Email => "email",
                RawFieldName::Web => "web",
                RawFieldName::Twitter => "twitter",
                RawFieldName::Matrix => "matrix",
            }
        })
    }
}

impl FromStr for RawFieldName {
    type Err = Response;

    fn from_str(s: &str) -> Result<Self> {
        // Convenience handler.
        let s = s.trim().replace("-", "").replace("_", "").to_lowercase();

        let f = match s.as_str() {
            "legalname" => RawFieldName::LegalName,
            "displayname" => RawFieldName::DisplayName,
            "email" => RawFieldName::Email,
            "web" => RawFieldName::Web,
            "twitter" => RawFieldName::Twitter,
            "matrix" => RawFieldName::Matrix,
            _ => return Err(Response::InvalidSyntax(Some(s.to_string()))),
        };

        Ok(f)
    }
}

pub async fn process_admin<'a>(db: &'a Database, command: Command) -> Response {
    let local = |db: &'a Database, command: Command| async move {
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
