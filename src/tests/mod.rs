use crate::database::Database;
use crate::adapters::AdapterListener;
use crate::adapters::tests::MessageInjector;
use crate::actors::api::run_rest_api_server_blocking;

mod api_judgement_state;

async fn new_env() -> (Database, MessageInjector) {
    use rand::{thread_rng, Rng};

    // Configure MongoDb database for testing.
    let random: [u8; 32] = thread_rng().gen();
    let db = Database::new(
        "mongodb://localhost:27017/",
        &format!(
            "registrar_test_{}",
            String::from_utf8(random.to_vec()).unwrap()
        ),
    )
    .await
    .unwrap();

    // Configure API
    let port = thread_rng().gen_range(1025, 65_535);
    run_rest_api_server_blocking(&format!("localhost:{}", port), db.clone()).await.unwrap();

    // Setup message verifier and injector.
    let injector = MessageInjector::new();
    let listener = AdapterListener::new(db.clone()).await;
    listener.start_message_adapter(injector.clone(), 1).await;

    (db, injector)
}
