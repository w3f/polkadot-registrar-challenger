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

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
struct AddAccountState {
    state: StateWrapper,
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
// TODO
struct DeleteAccountState {
    state: IdentityState,
}

#[derive(Default)]
pub struct WsAccountStatusSession {
    subscribers: HashMap<
        NetworkAddress,
        (
            Option<IdentityState>,
            Vec<Recipient<MessageResult<StateWrapper>>>,
        ),
    >,
}

impl WsAccountStatusSession {
    // Handle account state subscriptions from clients.
    fn handle_new_account_subscription(
        &mut self,
        net_address: NetworkAddress,
        recipient: Recipient<MessageResult<StateWrapper>>,
    ) {
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
                // dropped (handled by early `return`'s).
                recipients.push(recipient.clone());
            })
            .or_insert_with(|| {
                if recipient
                    .do_send(MessageResult::Err(
                        ErrorMessage::no_pending_judgement_request(REGISTRAR_IDX),
                    ))
                    .is_err()
                {
                    (None, vec![])
                } else {
                    // Only insert the recipient if the connection has not been
                    // dropped.
                    (None, vec![recipient])
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
                    self.handle_new_account_subscription(net_address, ctx.address().recipient());
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

// Handle added account states, created by the event store listener.
impl Handler<AddAccountState> for WsAccountStatusSession {
    type Result = ();

    fn handle(&mut self, msg: AddAccountState, _ctx: &mut Self::Context) -> Self::Result {
        let identity = msg.state;

        self.subscribers
            .entry(identity.state.net_address.clone())
            .and_modify(|(state, recipients)| {
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
