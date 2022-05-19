use crate::adapters::tests::MessageInjector;
use crate::adapters::AdapterListener;
use crate::api::{JsonResult, ResponseAccountState};
use crate::connector::{AccountType, JudgementRequest, WatcherMessage};
use crate::database::Database;
use crate::notifier::run_session_notifier;
use crate::primitives::{IdentityContext, IdentityFieldValue};
use crate::{api::tests::run_test_server, connector::tests::ConnectorMocker};
use actix_codec::{AsyncRead, AsyncWrite, Framed};
use actix_http::ws::Codec;
use actix_http::ws::{Frame, ProtocolError};
use actix_test::TestServer;
use actix_web_actors::ws::Message;
use futures::{FutureExt, SinkExt, StreamExt};
use rand::{thread_rng, Rng};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::time::{sleep, Duration};

mod api_judgement_state;
mod background_tasks;
mod display_name_verification;
mod explicit;
mod live_mocker;
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

pub async fn subscribe_context(
    stream: &mut Framed<impl AsyncRead + AsyncWrite + std::marker::Unpin, Codec>,
    context: IdentityContext,
) -> JsonResult<ResponseAccountState> {
    stream.send(context.to_ws()).await.unwrap();
    let mut resp = stream.next().await.into();

    while matches!(resp, JsonResult::Err(_)) {
        {
            sleep(Duration::from_millis(500)).await;
            resp = stream.next().await.into();
        }
    }

    resp
}

pub fn alice_judgement_request() -> WatcherMessage {
    WatcherMessage::new_judgement_request(JudgementRequest::alice())
}

pub fn bob_judgement_request() -> WatcherMessage {
    WatcherMessage::new_judgement_request(JudgementRequest::bob())
}

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
        run_session_notifier(t_db, actor).await;
    });

    // Setup connector mocker
    let connector = ConnectorMocker::new(db.clone());

    //(server, connector, injector)
    (db, connector, server, injector)
}
