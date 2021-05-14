use super::JsonResult;
use crate::database::Database;
use crate::primitives::{IdentityContext, JudgementState, NotificationMessage};
use actix::prelude::*;
use parity_scale_codec::Joiner;
use std::sync::Arc;
use tokio::sync::RwLock;
use actix::prelude::*;
use actix_broker::{Broker, BrokerIssue, BrokerSubscribe};
use actix_web_actors::ws;
use serde::Serialize;
use std::collections::HashMap;

// TODO: Set via config.
pub const REGISTRAR_IDX: usize = 0;

type Subscriber = Recipient<JsonResult<ResponseAccountState>>;

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
pub struct SubscribeAccountState {
    pub subscriber: Subscriber,
    pub id_context: IdentityContext,
}

#[derive(Clone, Debug, Message, Serialize)]
#[rtype(result = "()")]
pub struct NotifyAccountState {
    pub state: JudgementState,
    pub notifications: Vec<NotificationMessage>,
}

// Identical to `NotifyAccountState`, but gets sent from the server to the
// session for type-safety purposes.
#[derive(Clone, Debug, Message, Serialize)]
#[rtype(result = "()")]
pub struct ResponseAccountState {
    pub state: JudgementState,
    pub notifications: Vec<NotificationMessage>,
}

impl ResponseAccountState {
    fn with_no_notifications(state: JudgementState) -> Self {
        ResponseAccountState {
            state: state,
            notifications: vec![],
        }
    }
}

impl From<NotifyAccountState> for ResponseAccountState {
    fn from(val: NotifyAccountState) -> Self {
        ResponseAccountState {
            state: val.state,
            notifications: val.notifications,
        }
    }
}

#[derive(Default)]
pub struct LookupServer {
    // Database is wrapped in `Option' since implementing `SystemService`
    // requires this type to implement `Default` (which `Database` itself does not).
    db: Option<Database>,
    sessions: Arc<RwLock<HashMap<IdentityContext, Vec<Subscriber>>>>,
}

impl LookupServer {
    pub fn new(db: Database) -> Self {
        LookupServer {
            db: Some(db),
            sessions: Default::default(),
        }
    }
    fn get_db(&self) -> crate::Result<&Database> {
        self.db.as_ref().ok_or(anyhow!(
            "No database is configured for LookupServer registry service"
        ))
    }
}

impl SystemService for LookupServer {}
impl Supervised for LookupServer {}

impl Actor for LookupServer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // TODO: Use arbiter instead?
        self.subscribe_system_async::<NotifyAccountState>(ctx);
    }
}

impl Handler<SubscribeAccountState> for LookupServer {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: SubscribeAccountState, ctx: &mut Self::Context) -> Self::Result {
        let db = self.get_db().unwrap().clone();
        let sessions = Arc::clone(&self.sessions);

        Box::pin(
            async move {
                let (id, subscriber) = (msg.id_context, msg.subscriber);

                if let Some(state) = db.fetch_judgement_state(&id).await.unwrap() {
                    if subscriber
                        .do_send(JsonResult::Ok(ResponseAccountState::with_no_notifications(
                            state,
                        )))
                        .is_ok()
                    {
                        sessions
                            .write()
                            .await
                            .entry(id)
                            .and_modify(|subscribers| {
                                subscribers.push(subscriber.clone());
                            })
                            .or_insert(vec![subscriber]);
                    }
                } else {
                    // TODO: Set registrar index via config.
                    let _ = subscriber.do_send(JsonResult::Err(
                        "There is no judgement request from that account for registrar '0'"
                            .to_string(),
                    ));
                }
            }
            .into_actor(self),
        )
    }
}

impl Handler<NotifyAccountState> for LookupServer {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: NotifyAccountState, ctx: &mut Self::Context) -> Self::Result {
        let sessions = Arc::clone(&self.sessions);

        Box::pin(
            async move {
                // Move all subscribers into a temporary storage. Subscribers who
                // still have an active session open will be added back later.
                let mut to_reinsert = vec![];

                if let Some(subscribers) = sessions.read().await.get(&msg.state.context) {
                    // Notify each subscriber.
                    for subscriber in subscribers {
                        if subscriber
                            .do_send(JsonResult::Ok(msg.clone().into()))
                            .is_ok()
                        {
                            to_reinsert.push(subscriber.clone());
                        }
                    }
                }

                // Reinsert active subscribers back into storage.
                sessions
                    .write()
                    .await
                    .insert(msg.state.context, to_reinsert);
            }
            .into_actor(self),
        )
    }
}

#[derive(Default)]
pub struct WsAccountStatusSession;

impl Actor for WsAccountStatusSession {
    type Context = ws::WebsocketContext<Self>;
}

// Handle messages from the subscriber.
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsAccountStatusSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = if let Ok(msg) = msg {
            msg
        } else {
            ctx.stop();
            return;
        };

        match msg {
            ws::Message::Text(txt) => {
                if let Ok(context) = serde_json::from_str::<IdentityContext>(txt.as_str()) {
                    // Subscribe the the specified identity context.
                    LookupServer::from_registry()
                        .send(SubscribeAccountState {
                            subscriber: ctx.address().recipient(),
                            id_context: context,
                        })
                        .into_actor(self)
                        .then(|_, _, _| fut::ready(()))
                        .wait(ctx);
                } else {
                    // Invalid message type, inform caller.
                    match serde_json::to_string(&JsonResult::<()>::Err(
                        "Invalid message type".to_string(),
                    )) {
                        Ok(m) => ctx.text(m),
                        Err(err) => {
                            error!("Failed to serialize WS session message response: {:?}", err)
                        }
                    }
                }
            }
            ws::Message::Ping(b) => {
                ctx.pong(&b);
            }
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

impl<T: Serialize> Handler<JsonResult<T>> for WsAccountStatusSession {
    type Result = ();

    fn handle(&mut self, msg: JsonResult<T>, ctx: &mut Self::Context) -> Self::Result {
        match serde_json::to_string(&msg) {
            Ok(m) => ctx.text(m),
            Err(err) => error!("Failed to serialize WS session message response: {:?}", err),
        }
    }
}