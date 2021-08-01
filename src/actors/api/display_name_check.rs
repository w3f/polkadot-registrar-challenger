use super::JsonResult;
use crate::actors::connector::DisplayNameEntry;
use crate::database::Database;
use crate::{display_name::DisplayNameVerifier, DisplayNameConfig};
use actix::prelude::*;

pub struct DisplayNameChecker {
    verifier: DisplayNameVerifier,
}

impl Default for DisplayNameChecker {
    fn default() -> Self {
        panic!("DisplayNameChecker is not initialized");
    }
}

impl DisplayNameChecker {
    pub fn new(db: Database, config: DisplayNameConfig) -> Self {
        DisplayNameChecker {
            verifier: DisplayNameVerifier::new(db, config),
        }
    }
}

impl SystemService for DisplayNameChecker {}
impl Supervised for DisplayNameChecker {}

impl Actor for DisplayNameChecker {
    type Context = Context<Self>;
}

impl Handler<CheckDisplayName> for DisplayNameChecker {
    type Result = ResponseActFuture<Self, JsonResult<Outcome>>;

    fn handle(&mut self, msg: CheckDisplayName, _ctx: &mut Self::Context) -> Self::Result {
        let verifier = self.verifier.clone();

        Box::pin(
            async move {
                debug!("");
                verifier
                    .check_similarities(msg.check.display_name.as_str())
                    .await
                    .map(|violations| {
                        let outcome = if violations.is_empty() {
                            Outcome::Ok
                        } else {
                            Outcome::Violations(violations)
                        };

                        JsonResult::Ok(outcome)
                    })
                    .unwrap()
            }
            .into_actor(self),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Ok,
    Violations(Vec<DisplayNameEntry>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Message)]
#[rtype(result = "JsonResult<Outcome>")]
pub struct CheckDisplayName {
    pub check: DisplayNameEntry,
}
