use crate::actors::api::tests::run_test_server;
use crate::adapters::tests::MessageInjector;
use crate::adapters::AdapterListener;
use crate::database::Database;
use actix_http::Request;
use actix_web::dev::{Service, ServiceResponse};
use actix_web::Error as ActixError;

mod api_judgement_state;

async fn new_env() -> (
    Database,
    impl Service<Request = Request, Response = ServiceResponse, Error = ActixError>,
    MessageInjector,
) {
    use rand::{thread_rng, Rng};

    // Setup MongoDb database.
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

    // Setup API
    let api = run_test_server(db.clone()).await;

    // Setup message verifier and injector.
    let injector = MessageInjector::new();
    let listener = AdapterListener::new(db.clone()).await;
    listener.start_message_adapter(injector.clone(), 1).await;

    (db, api, injector)
}
