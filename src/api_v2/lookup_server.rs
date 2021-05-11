use super::JsonResult;
use crate::primitives::{IdentityContext, NotificationMessage, JudgementState};
use crate::Result;
use actix::prelude::*;
use actix_broker::BrokerSubscribe;
use std::{collections::HashMap, ops::Rem};

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
pub struct SubscribeAccountState {
    //recipient: Recipient<JsonResult<NotificationMessage>>,
    pub id_context: IdentityContext,
}

#[derive(Clone, Debug, Message)]
#[rtype(result = "()")]
pub struct NotifyAccountState {
    pub state: JudgementState,
    pub notifications: Vec<NotificationMessage>,
}

#[derive(Default)]
pub struct LookupServer {}

impl SystemService for LookupServer {}
impl Supervised for LookupServer {}

impl Actor for LookupServer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // TODO: Use arbiter instead?
        //self.subscribe_system_async::<AddIdentityState>(ctx);
    }
}

impl Handler<NotifyAccountState> for LookupServer {
    type Result = ();

    fn handle(&mut self, msg: NotifyAccountState, _ctx: &mut Self::Context) {
        unimplemented!()
    }
}
