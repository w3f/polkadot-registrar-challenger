use crate::primitives::{ChainAddress, ChainName, IdentityContext, JudgementStateBlanked};
use crate::Database;
use std::str::FromStr;

pub type Result<T> = std::result::Result<T, Response>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Command {
    Status(ChainAddress),
    Verify(ChainAddress, Vec<RawFieldName>),
    Help,
}

impl FromStr for Command {
    type Err = Response;

    fn from_str(s: &str) -> Result<Self> {
        // Convenience handler.
        let s = s.trim().replace("  ", " ");

        if s.starts_with("status") {
            let parts: Vec<&str> = s.split(' ').skip(1).collect();
            if parts.len() != 1 {
                return Err(Response::UnknownCommand);
            }

            Ok(Command::Status(ChainAddress::from(parts[0].to_string())))
        } else if s.starts_with("verify") {
            let parts: Vec<&str> = s.split(' ').skip(1).collect();
            if parts.len() < 2 {
                return Err(Response::UnknownCommand);
            }

            Ok(Command::Verify(
                ChainAddress::from(parts[0].to_string()),
                parts[1..]
                    .iter()
                    .map(|s| RawFieldName::from_str(s))
                    .collect::<Result<Vec<RawFieldName>>>()?,
            ))
        } else if s.starts_with("help") {
            let count = s.split(' ').count();

            if count > 1 {
                return Err(Response::UnknownCommand);
            }

            Ok(Command::Help)
        } else {
            Err(Response::UnknownCommand)
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Response {
    Status(JudgementStateBlanked),
    Verified(ChainAddress, Vec<RawFieldName>),
    UnknownCommand,
    IdentityNotFound,
    InvalidSyntax(Option<String>),
    FullyVerified(ChainAddress),
    InternalError,
    Help,
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Response::Status(state) => serde_json::to_string_pretty(state).unwrap(),
            Response::Verified(_, fields) => {
                format!("Verified the following fields: {}", {
                    let mut all = String::new();
                    for field in fields {
                        all.push_str(&format!("{}, ", field));
                    }

                    // Remove `, ` suffix.
                    all.pop();
                    all.pop();

                    all
                })
            }
            Response::UnknownCommand => "The provided command is unknown".to_string(),
            Response::IdentityNotFound => {
                "Identity was not found or invalid query executed".to_string()
            }
            Response::InvalidSyntax(input) => {
                format!(
                    "Invalid input{}",
                    match input {
                        Some(input) => format!(" '{}'", input),
                        None => "".to_string(),
                    }
                )
            }
            Response::InternalError => {
                "An internal error occured. Please contact the architects.".to_string()
            }
            Response::Help => "\
                status <ADDR>\t\t\tShow the current verification status of the specified address.\n\
                verify <ADDR> <FIELD>...\tVerify one or multiple fields of the specified address.\n\
                "
            .to_string(),
            Response::FullyVerified(_) => {
                "Identity has been fully verified. The extrinsic will be submitted in a couple of minutes".to_string()
            },
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
    // Represents the full identity
    All,
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
                RawFieldName::All => "all",
            }
        })
    }
}

impl FromStr for RawFieldName {
    type Err = Response;

    fn from_str(s: &str) -> Result<Self> {
        // Convenience handler.
        let s = s.trim().replace('-', "").replace('_', "").to_lowercase();

        let f = match s.as_str() {
            "legalname" => RawFieldName::LegalName,
            "displayname" => RawFieldName::DisplayName,
            "email" => RawFieldName::Email,
            "web" => RawFieldName::Web,
            "twitter" => RawFieldName::Twitter,
            "matrix" => RawFieldName::Matrix,
            "all" => RawFieldName::All,
            _ => return Err(Response::InvalidSyntax(Some(s.to_string()))),
        };

        Ok(f)
    }
}

#[allow(clippy::needless_lifetimes)]
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

                // Check if _all_ should be verified (respectively the full identity)
                #[allow(clippy::collapsible_if)]
                if fields.iter().any(|f| matches!(f, RawFieldName::All)) {
                    if db.full_manual_verification(&context).await? {
                        return Ok(Response::FullyVerified(addr));
                    } else {
                        return Ok(Response::IdentityNotFound);
                    }
                }

                // Verify each passed on field.
                for field in &fields {
                    if db
                        .verify_manually(&context, field, true, None)
                        .await?
                        .is_none()
                    {
                        return Ok(Response::IdentityNotFound);
                    }
                }

                Ok(Response::Verified(addr, fields))
            }
            Command::Help => Ok(Response::Help),
        }
    };

    let res: crate::Result<Response> = local(db, command).await;
    match res {
        Ok(resp) => resp,
        Err(err) => {
            error!("Admin tool: {:?}", err);
            dbg!(err);
            Response::InternalError
        }
    }
}

/// Convenience function for creating a full identity context when only the
/// address itself is present. Only supports Kusama and Polkadot for now.
pub fn create_context(address: ChainAddress) -> IdentityContext {
    let chain = if address.as_str().starts_with('1') {
        ChainName::Polkadot
    } else {
        ChainName::Kusama
    };

    IdentityContext { address, chain }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::JudgementState;

    #[test]
    fn command_status() {
        let resp = Command::from_str("status Alice").unwrap();
        assert_eq!(
            resp,
            Command::Status(ChainAddress::from("Alice".to_string()))
        );

        let resp = Command::from_str("status  Alice").unwrap();
        assert_eq!(
            resp,
            Command::Status(ChainAddress::from("Alice".to_string()))
        );

        let resp = Command::from_str("status");
        assert!(resp.is_err())
    }

    #[test]
    fn command_verify() {
        let resp = Command::from_str("verify Alice email").unwrap();
        assert_eq!(
            resp,
            Command::Verify(
                ChainAddress::from("Alice".to_string()),
                vec![RawFieldName::Email]
            )
        );

        let resp = Command::from_str("verify Alice email displayname").unwrap();
        assert_eq!(
            resp,
            Command::Verify(
                ChainAddress::from("Alice".to_string()),
                vec![RawFieldName::Email, RawFieldName::DisplayName]
            )
        );

        let resp = Command::from_str("verify Alice email display_name").unwrap();
        assert_eq!(
            resp,
            Command::Verify(
                ChainAddress::from("Alice".to_string()),
                vec![RawFieldName::Email, RawFieldName::DisplayName]
            )
        );

        let resp = Command::from_str("verify Alice all").unwrap();
        assert_eq!(
            resp,
            Command::Verify(
                ChainAddress::from("Alice".to_string()),
                vec![RawFieldName::All]
            )
        );

        let resp = Command::from_str("verify Alice");
        assert!(resp.is_err());
    }

    #[test]
    fn command_help() {
        let resp = Command::from_str("help").unwrap();
        assert_eq!(resp, Command::Help);

        let resp = Command::from_str(" help  ").unwrap();
        assert_eq!(resp, Command::Help);

        let resp = Command::from_str("help stuff");
        assert!(resp.is_err());
    }

    #[test]
    #[ignore]
    fn response_status_debug() {
        let resp = Response::Status(JudgementState::alice().into());
        println!("{}", resp);
    }

    #[test]
    #[ignore]
    fn response_help_debug() {
        let resp = Response::Help;
        println!("{}", resp);
    }
}
