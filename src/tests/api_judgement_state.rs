use super::new_env;
use crate::database::Database;

#[actix_rt::test]
async fn add_identity() {
    let (db, injector) = new_env().await;
}
