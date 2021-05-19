use super::new_db;
use crate::adapters::tests::MessageInjector;
use crate::database::Database;

#[actix_rt::test]
async fn add_identity() {
    let injector = MessageInjector::new();
    let db = new_db().await;
}
