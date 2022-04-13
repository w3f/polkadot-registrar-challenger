use super::JsonResult;
use crate::actors::connector::DisplayNameEntry;
use crate::database::Database;
use crate::primitives::ChainName;
use crate::{display_name::DisplayNameVerifier, DisplayNameConfig};
use actix::prelude::*;
use actix_web::{web, HttpResponse};

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
                trace!("Received a similarities check: {:?}", msg);
                verifier
                    .check_similarities(msg.check.as_str(), msg.chain, None)
                    .await
                    .map(|violations| {
                        let outcome = if violations.is_empty() {
                            Outcome::Ok
                        } else {
                            Outcome::Violations(violations)
                        };

                        JsonResult::Ok(outcome)
                    })
                    .map_err(|err| {
                        error!("Failed to check for display name similarities: {:?}", err)
                    })
                    .unwrap_or_else(|_| JsonResult::Err("Backend error, contact admin".to_string()))
            }
            .into_actor(self),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum Outcome {
    Ok,
    Violations(Vec<DisplayNameEntry>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Message)]
#[rtype(result = "JsonResult<Outcome>")]
pub struct CheckDisplayName {
    pub check: String,
    pub chain: ChainName,
}

pub async fn check_display_name(req: web::Json<CheckDisplayName>) -> HttpResponse {
    HttpResponse::Ok().json(
        DisplayNameChecker::from_registry()
            .send(req.into_inner())
            .await
            .unwrap(),
    )
}
