use super::{new_env, ToWsMessage};
use crate::database::Database;
use crate::primitives::{IdentityContext, JudgementState};
use futures::{SinkExt, StreamExt};

#[actix::test]
async fn add_identity() {
    let (db, mut api, injector) = new_env().await;

    let alice = JudgementState::alice();
    db.add_judgement_request(alice).await.unwrap();

    let mut stream = api.ws_at("/api/account_status").await.unwrap();
    stream.send(IdentityContext::alice().to_ws()).await.unwrap();
    let x = stream.next().await;
    println!("{:?}", x);
}
