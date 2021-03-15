use crate::event::{ErrorMessage, StateWrapper};
use crate::manager::{IdentityState, NetworkAddress};
use actix::prelude::*;
use actix_broker::{BrokerIssue, BrokerSubscribe};
use actix_web_actors::ws;
use std::collections::HashMap;

// TODO: Set via config.
const REGISTRAR_IDX: usize = 0;

type MessageResult<T> = Result<T, ErrorMessage>;

struct WsAccountStatusSession;

impl Actor for WsAccountStatusSession {
    type Context = ws::WebsocketContext<Self>;
}

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
                if let Ok(net_address) = serde_json::from_str::<NetworkAddress>(txt.as_str()) {
                    self.issue_system_async(SubscribeAccountStatus {
                        net_address: net_address,
                    });
                } else {
                    ctx.text("Invalid message type");
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

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
struct SubscribeAccountStatus {
    net_address: NetworkAddress,
}

pub struct WsAccountStatusServer {
    watcher: HashMap<NetworkAddress, (IdentityState, Vec<Recipient<SubscribeAccountStatus>>)>,
}

impl Actor for WsAccountStatusServer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<SubscribeAccountStatus>(ctx);
    }
}

impl Handler<SubscribeAccountStatus> for WsAccountStatusServer {
    type Result = ();

    fn handle(&mut self, msg: SubscribeAccountStatus, ctx: &mut Self::Context) -> Self::Result {
        if let Some((state, _)) = self.watcher.get_mut(&msg.net_address) {
            //Ok(StateWrapper::from(state.clone()))
        } else {
            //Err(ErrorMessage::no_pending_judgement_request(REGISTRAR_IDX))
        }
    }
}
