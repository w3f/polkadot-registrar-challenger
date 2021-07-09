use crate::actors::api::tests::run_test_server;
use crate::actors::api::JsonResult;
use crate::adapters::tests::MessageInjector;
use crate::adapters::AdapterListener;
use crate::database::Database;
use crate::notifier::SessionNotifier;
use actix_http::ws::{Frame, ProtocolError};
use actix_test::TestServer;
use actix_web_actors::ws::Message;
use futures::FutureExt;
use rand::{thread_rng, Rng};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::future::Future;
use tokio::time::{sleep, Duration};

// mod api_judgement_state;

trait ToWsMessage {
    fn to_ws(&self) -> Message;
}

impl<T: Serialize> ToWsMessage for T {
    fn to_ws(&self) -> Message {
        Message::Text(serde_json::to_string(&self).unwrap().into())
    }
}

impl<T: DeserializeOwned> From<Option<Result<Frame, ProtocolError>>> for JsonResult<T> {
    fn from(val: Option<Result<Frame, ProtocolError>>) -> Self {
        match val.unwrap().unwrap() {
            Frame::Text(t) => serde_json::from_slice::<JsonResult<T>>(&t).unwrap(),
            _ => panic!(),
        }
    }
}

async fn wait() {
    sleep(Duration::from_secs(1)).await;
}

async fn new_env() -> (Database, TestServer, MessageInjector) {
    // Setup MongoDb database.
    let random: u32 = thread_rng().gen_range(u32::MIN, u32::MAX);
    let db = Database::new(
        "mongodb://localhost:27017/",
        &format!("registrar_test_{}", random),
    )
    .await
    .unwrap();

    // Setup API
    let (server, actor) = run_test_server(db.clone()).await;

    // Setup message verifier and injector.
    let injector = MessageInjector::new();
    let listener = AdapterListener::new(db.clone()).await;
    listener.start_message_adapter(injector.clone(), 1).await;

    let t_db = db.clone();
    actix::spawn(async move {
        SessionNotifier::new(t_db, actor).run_blocking().await;
    });

    (db, server, injector)
}
