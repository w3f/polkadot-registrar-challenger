use crate::actors::api::JsonResult;
use crate::actors::connector::{JudgementRequest, WatcherMessage, AccountType};
use crate::actors::{api::tests::run_test_server, connector::tests::ConnectorMocker};
use crate::adapters::tests::MessageInjector;
use crate::adapters::AdapterListener;
use crate::database::Database;
use crate::notifier::SessionNotifier;
use crate::primitives::{IdentityFieldValue, JudgementState};
use actix_http::ws::{Frame, ProtocolError};
use actix_test::TestServer;
use actix_web_actors::ws::Message;
use rand::{thread_rng, Rng};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::time::{sleep, Duration};
use std::collections::HashMap;

mod api_judgement_state;
mod display_name_verification;
mod explicit;
mod process_admin_cmds;

// Convenience type
pub type F = IdentityFieldValue;

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

pub fn judgement_request_message(state: JudgementState) -> WatcherMessage {
    WatcherMessage::new_judgement_request(JudgementRequest {
        address: state.context.address,
        accounts: {
            let mut accounts = HashMap::new();

            for field in state.fields {
                let (a, v) = match field.value {
                    IdentityFieldValue::DisplayName(n) => (AccountType::DisplayName, n),
                    IdentityFieldValue::Email(n) => (AccountType::Email, n),
                    IdentityFieldValue::Twitter(n) => (AccountType::Twitter, n),
                    IdentityFieldValue::Matrix(n) => (AccountType::Matrix, n),
                    _ => panic!(""),
                };

                accounts.insert(a, v);
            }

            accounts
        }
    })
}

/*
pub struct JudgementRequest {
    pub address: ChainAddress,
    pub accounts: HashMap<AccountType, String>,
}
*/

// async fn new_env() -> (TestServer, ConnectorMocker, MessageInjector) {
async fn new_env() -> (Database, ConnectorMocker, TestServer, MessageInjector) {
    // Setup MongoDb database.
    let random: u32 = thread_rng().gen_range(u32::MIN..u32::MAX);
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

    // Setup connector mocker
    let connector = ConnectorMocker::new(db.clone());

    // Give some time to start up.
    sleep(Duration::from_secs(3)).await;

    //(server, connector, injector)
    (db, connector, server, injector)
}
