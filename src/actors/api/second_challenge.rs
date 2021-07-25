use super::JsonResult;
use crate::database::Database;
use crate::primitives::IdentityFieldValue;
use actix::prelude::*;
use actix_web::{web, HttpResponse};

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
    type Result = ResponseActFuture<Self, JsonResult<bool>>;

    fn handle(&mut self, msg: VerifyChallenge, _ctx: &mut Self::Context) -> Self::Result {
        let mut db = self.get_db().unwrap().clone();

        Box::pin(
            async move {
                db.verify_second_challenge(msg)
                    .await
                    .map(|b| JsonResult::Ok(b))
                    .unwrap()
            }
            .into_actor(self),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Message)]
#[rtype(result = "JsonResult<bool>")]
pub struct VerifyChallenge {
    pub entry: IdentityFieldValue,
    pub challenge: String,
}

pub async fn verify_second_challenge(req: web::Json<VerifyChallenge>) -> HttpResponse {
    HttpResponse::Ok().json(
        SecondChallengeVerifier::from_registry()
            .send(req.into_inner())
            .await
            .unwrap(),
    )
}
