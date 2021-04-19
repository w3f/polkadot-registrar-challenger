use super::MessageResult;
use super::lookup_server::{LookupServer, RequestAccountState};
use crate::event::{ErrorMessage, StateWrapper};
use crate::manager::{IdentityState, NetworkAddress};
use actix::prelude::*;
use actix::SystemService;
use actix_broker::{Broker, BrokerIssue, BrokerSubscribe};
use actix_web_actors::ws;
use std::collections::HashMap;

// TODO: Set via config.
const REGISTRAR_IDX: usize = 0;

#[derive(Default)]
pub struct WsAccountStatusSession {
    identities: HashMap<NetworkAddress, Option<IdentityState>>,
}

impl WsAccountStatusSession {
    // Handle account state subscriptions from clients.
    fn handle_new_account_subscription(
        &mut self,
        net_address: NetworkAddress,
        recipient: Recipient<MessageResult<StateWrapper>>,
    ) {
        self.identities.get(&net_address).map(|state| {
            if let Some(state) = state {
                if recipient
                    .do_send(MessageResult::Ok(StateWrapper::from(state.clone())))
                    .is_err()
                {
                    return;
                };
            } else {
                if recipient
                    .do_send(MessageResult::Err(
                        ErrorMessage::no_pending_judgement_request(REGISTRAR_IDX),
                    ))
                    .is_err()
                {
                    return;
                };
            }
        });
    }
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
                    //self.handle_new_account_subscription(net_address, ctx.address().recipient());
                    LookupServer::from_registry()
                        .send(RequestAccountState {
                            net_address: net_address,
                        })
                        .into_actor(self)
                        .wait(ctx);
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
impl Handler<MessageResult<StateWrapper>> for WsAccountStatusSession {
    type Result = ();

    fn handle(&mut self, msg: MessageResult<StateWrapper>, ctx: &mut Self::Context) {
        if let Ok(payload) = serde_json::to_string(&msg) {
            ctx.text(payload)
        } else {
            error!("Failed to send account state message to client: deserialization error");
        }
    }
}
