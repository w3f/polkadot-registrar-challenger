use super::lookup_server::{LookupServer, RequestAccountState};
use super::JsonResult;
use crate::event::{ErrorMessage, StateWrapper};
use crate::manager::{IdentityState, NetworkAddress};
use actix::prelude::*;
use actix_broker::{Broker, BrokerIssue, BrokerSubscribe};
use actix_web_actors::ws;
use std::collections::HashMap;

// TODO: Set via config.
pub const REGISTRAR_IDX: usize = 0;

#[derive(Default)]
pub struct WsAccountStatusSession {
    watch: Option<NetworkAddress>,
}

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
                if let Ok(net_address) = serde_json::from_str::<NetworkAddress>(txt.as_str()) {
                    // Specify which network address to watch for.
                    self.watch = Some(net_address.clone());

                    LookupServer::from_registry()
                        .send(RequestAccountState {
                            recipient: ctx.address().recipient(),
                            net_address: net_address,
                        })
                        .into_actor(self);
                } else {
                    // TODO: Should be `MessageResult`
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

// Response message received from the server is sent directly to the subscriber.
impl Handler<JsonResult<StateWrapper>> for WsAccountStatusSession {
    type Result = ();

    fn handle(&mut self, msg: JsonResult<StateWrapper>, ctx: &mut Self::Context) {
        unimplemented!()
    }
}
