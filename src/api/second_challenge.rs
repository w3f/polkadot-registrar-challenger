use super::JsonResult;
use crate::database::Database;
use crate::primitives::IdentityFieldValue;
use actix::prelude::*;
use actix_web::{web, HttpResponse};

pub struct SecondChallengeVerifier {
    db: Database,
}

impl Default for SecondChallengeVerifier {
    fn default() -> Self {
        panic!("SecondChallengeVerifier is not initialized");
    }
}

impl SecondChallengeVerifier {
    pub fn new(db: Database) -> Self {
        SecondChallengeVerifier { db }
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
        let mut db = self.db.clone();

        Box::pin(
            async move {
                debug!("Received second challenge: {:?}", msg);
                db.verify_second_challenge(msg)
                    .await
                    .map(JsonResult::Ok)
                    .unwrap_or_else(|_| JsonResult::Err("Backend error, contact admin".to_string()))
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
