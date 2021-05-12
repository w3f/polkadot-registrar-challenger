use super::lookup_server::LookupServer;
use super::JsonResult;
use crate::primitives::IdentityContext;
use actix::prelude::*;
use actix_broker::{Broker, BrokerIssue, BrokerSubscribe};
use actix_web_actors::ws;
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
                let x = LookupServer::from_registry();
                /*
                if let Ok(net_address) = serde_json::from_str::<IdentityContext>(txt.as_str()) {
                    // Subscribe the the specified network address.
                    LookupServer::from_registry()
                        .send(RequestAccountState {
                            recipient: ctx.address().recipient(),
                            net_address: net_address,
                        })
                        .into_actor(self)
                        .then(|res, _, ctx| {
                            // Handle response and notify client about current state of the identity.
                            if let Ok(state) = res {
                                if let Ok(txt) = serde_json::to_string(&JsonResult::Ok(
                                    state
                                )) {
                                    ctx.text(txt);
                                } else {
                                    error!("Failed to deserialize identity state response on subscription request");
                                }
                            }

                            fut::ready(())
                        })
                        .wait(ctx);
                } else {
                    // TODO: Should be `MessageResult`
                    ctx.text("Invalid message type");
                }
                */
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
