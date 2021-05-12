use super::lookup_server::{LookupServer, SubscribeAccountState};
use super::JsonResult;
use crate::primitives::IdentityContext;
use actix::prelude::*;
use actix_broker::{Broker, BrokerIssue, BrokerSubscribe};
use actix_web_actors::ws;
use serde::Serialize;
use std::collections::HashMap;

// TODO: Set via config.
pub const REGISTRAR_IDX: usize = 0;

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
