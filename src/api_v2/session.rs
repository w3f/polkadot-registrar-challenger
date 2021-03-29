use crate::event::{ErrorMessage, StateWrapper};
use crate::manager::{IdentityState, NetworkAddress};
use actix::prelude::*;
use actix_broker::{BrokerIssue, BrokerSubscribe};
use actix_web_actors::ws;
use std::collections::HashMap;

// TODO: Set via config.
const REGISTRAR_IDX: usize = 0;

#[derive(Debug, Clone, Serialize, Message)]
#[rtype(result = "()")]
#[serde(untagged)]
enum MessageResult<T> {
    Ok(T),
    Err(ErrorMessage),
}

struct WsAccountStatusSession;

impl Actor for WsAccountStatusSession {
    type Context = ws::WebsocketContext<Self>;
}

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
                        recipient: ctx.address().recipient(),
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
    recipient: Recipient<MessageResult<StateWrapper>>,
    net_address: NetworkAddress,
}

pub struct WsAccountStatusServer {
    subscribers: HashMap<
        NetworkAddress,
        (
            Option<IdentityState>,
            Vec<Recipient<MessageResult<StateWrapper>>>,
        ),
    >,
}

impl Actor for WsAccountStatusServer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.subscribe_system_async::<SubscribeAccountStatus>(ctx);
    }
}

impl Handler<SubscribeAccountStatus> for WsAccountStatusServer {
    type Result = ();

    fn handle(&mut self, msg: SubscribeAccountStatus, _ctx: &mut Self::Context) -> Self::Result {
        let (recipient, net_address) = (msg.recipient, msg.net_address);

        self.subscribers
            .entry(net_address)
            .and_modify(|(state, recipients)| {
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

                // Only insert the recipient if the connection has not been
                // dropped (handled by early `return`).
                recipients.push(recipient.clone());
            })
            .or_insert((None, vec![recipient]));
    }
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
struct AddAccountState {
    state: StateWrapper,
}

impl Handler<AddAccountState> for WsAccountStatusServer {
    type Result = ();

    fn handle(&mut self, msg: AddAccountState, _ctx: &mut Self::Context) -> Self::Result {
        let identity = msg.state;

        self.subscribers
            .entry(identity.state.net_address.clone())
            .and_modify(|(state, recipients)| {
                if state.is_some() {
                    debug!(
                        "Account state for {} has already been added to the websocket handler",
                        identity.state.net_address.address_str()
                    );
                    return;
                }

                // Set account state.
                *state = Some(identity.state.clone());

                // Notify each subscriber
                let to_notify = std::mem::take(recipients);
                for recipient in to_notify {
                    if recipient
                        .do_send(MessageResult::Ok(identity.clone()))
                        .is_ok()
                    {
                        // The recipient still has a subscription open, so add
                        // them back for future notifications.
                        recipients.push(recipient);
                    }
                }
            })
            .or_insert((Some(identity.state), vec![]));
    }
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
// TODO
struct DeleteAccountState {
    state: IdentityState,
}
