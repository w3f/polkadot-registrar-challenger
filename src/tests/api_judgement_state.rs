use super::new_env;
use crate::database::Database;
use crate::primitives::JudgementState;

#[actix_rt::test]
async fn add_identity() {
    let (db, injector) = new_env().await;

    let alice = JudgementState::alice();
    db.add_judgement_request(alice).await.unwrap();
}
