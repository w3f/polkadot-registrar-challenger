use super::JsonResult;
use crate::database::Database;
use crate::primitives::{IdentityContext, JudgementState, NotificationMessage};
use crate::Result;
use actix::prelude::*;
use actix_broker::BrokerSubscribe;
use parity_scale_codec::Joiner;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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
    fn get_db(&self) -> Result<&Database> {
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
