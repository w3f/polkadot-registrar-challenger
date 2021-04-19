use crate::event::{ErrorMessage, StateWrapper};
use crate::manager::{IdentityState, NetworkAddress};
use actix::prelude::*;
use actix_broker::{Broker, BrokerIssue, BrokerSubscribe};
use std::collections::HashMap;

#[derive(Default)]
pub struct LookupServer {
    identities: HashMap<NetworkAddress, IdentityState>,
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub struct RequestAccountState {
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
    type Result = ();

    fn handle(&mut self, msg: RequestAccountState, _ctx: &mut Self::Context) -> Self::Result {
        unimplemented!()
    }
}

// Handle added account states, created by the event store listener.
impl Handler<AddAccountState> for LookupServer {
    type Result = ();

    fn handle(&mut self, msg: AddAccountState, _ctx: &mut Self::Context) -> Self::Result {
        /*
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
            */
    }
}
