use super::JsonResult;
use crate::database::Database;
use crate::primitives::IdentityFieldValue;
use actix::prelude::*;
use actix_web::{get, web, App, Error, HttpResponse, HttpServer};

#[derive(Default)]
pub struct SecondChallengeVerifier {
    // Database is wrapped in `Option' since implementing `SystemService`
    // requires this type to implement `Default` (which `Database` itself does not).
    db: Option<Database>,
}

impl SecondChallengeVerifier {
    pub fn new(db: Database) -> Self {
        SecondChallengeVerifier { db: Some(db) }
    }
    fn get_db(&self) -> crate::Result<&Database> {
        self.db.as_ref().ok_or(anyhow!(
            "No database is configured for SecondChallengeVerifier registry service"
        ))
    }
}

impl SystemService for SecondChallengeVerifier {}
impl Supervised for SecondChallengeVerifier {}

impl Actor for SecondChallengeVerifier {
    type Context = Context<Self>;
}

impl Handler<VerifyChallenge> for SecondChallengeVerifier {
    type Result = ResponseActFuture<Self, bool>;

    fn handle(&mut self, msg: VerifyChallenge, ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Message)]
#[rtype(result = "bool")]
pub struct VerifyChallenge {
    pub entry: IdentityFieldValue,
    pub challenge: String,
}

#[post("/api/verify_second_challenge")]
async fn verify_second_challenge(req: web::Json<VerifyChallenge>) -> HttpResponse {
    unimplemented!()
}
