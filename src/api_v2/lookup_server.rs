use super::session::{StateNotification, REGISTRAR_IDX};
use super::JsonResult;
use crate::event::{ErrorMessage, StateWrapper};
use crate::manager::{IdentityState, NetworkAddress};
use actix::prelude::*;
use actix_broker::{Broker, BrokerIssue, BrokerSubscribe};
use std::collections::HashMap;

pub type RecipientAccountState = Recipient<StateNotification>;

// TODO: Rename (reference "subscribe")
#[derive(Debug, Clone, Message)]
#[rtype(result = "JsonResult<IdentityState>")]
pub struct RequestAccountState {
    pub recipient: RecipientAccountState,
    pub net_address: NetworkAddress,
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
pub struct LookupServer {
    identities: HashMap<NetworkAddress, IdentityState>,
    listeners: HashMap<NetworkAddress, Vec<RecipientAccountState>>,
}

impl LookupServer {
    fn subscribe_net_address(
        &mut self,
        net_address: NetworkAddress,
        recipient: RecipientAccountState,
    ) {
        self.listeners
            .entry(net_address)
            .and_modify(|recipients| recipients.push(recipient.clone()))
            .or_insert(vec![recipient]);
    }
    fn update_state(&mut self, state: StateWrapper) {
        let net_address = state.state.net_address.clone();

        // Notify clients.
        if let Some(listeners) = self.listeners.get_mut(&net_address) {
            // Temporary storage for recipients (to get around Rust's borrowing rules).
            let mut tmp = vec![];

            // Notify the subscriber and, if still active, add them back for
            // future notifications.
            for recipient in listeners.drain(..) {
                if recipient
                    .do_send(StateNotification::from(state.clone()))
                    .is_ok()
                {
                    tmp.push(recipient);
                }
            }

            // Add back the recipients.
            *listeners = tmp;
        }

        // Update state (discard the notifications).
        self.identities.insert(net_address, state.state);
    }
}

impl SystemService for LookupServer {}
impl Supervised for LookupServer {}

impl Actor for LookupServer {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // TODO: Use arbiter instead?
        self.subscribe_system_async::<AddAccountState>(ctx);
    }
}

impl Handler<RequestAccountState> for LookupServer {
    type Result = JsonResult<IdentityState>;

    fn handle(&mut self, msg: RequestAccountState, _ctx: &mut Self::Context) -> Self::Result {
        let (recipient, net_address) = (msg.recipient, msg.net_address);

        // Add client as subscriber.
        self.subscribe_net_address(net_address.clone(), recipient);

        // Return current state.
        if let Some(state) = self.identities.get(&net_address) {
            JsonResult::Ok(state.clone())
        } else {
            JsonResult::<IdentityState>::Err(ErrorMessage::no_pending_judgement_request(
                REGISTRAR_IDX,
            ))
        }
    }
}

// Handle added account states, created by the event store listener.
impl Handler<AddAccountState> for LookupServer {
    type Result = ();

    fn handle(&mut self, msg: AddAccountState, _ctx: &mut Self::Context) -> Self::Result {
        self.update_state(msg.state)
    }
}
