use super::session::{StateNotification, REGISTRAR_IDX};
use super::JsonResult;
use crate::event::{ErrorMessage, Notification, StateWrapper};
use crate::event::{FieldStatusVerified, IdentityInserted};
use crate::manager::{IdentityManager, IdentityState, NetworkAddress};
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
// TODO
struct DeleteAccountState {
    state: IdentityState,
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum AddAccountState {
    // A new identity has been inserted.
    IdentityInserted(IdentityInserted),
    // A specific field was verified.
    FieldStatusVerified(FieldStatusVerified),
}

#[derive(Default)]
pub struct LookupServer {
    identities: HashMap<NetworkAddress, IdentityState>,
    manager: IdentityManager,
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
    fn update_identity(&mut self, update: AddAccountState) {
        let (net_address, state) = match update {
            AddAccountState::IdentityInserted(inserted) => {
                // Insert new identity into the manager.
                self.manager.insert_identity(inserted.clone());

                (
                    inserted.identity.net_address.clone(),
                    StateWrapper::newly_inserted_notification(inserted),
                )
            }
            AddAccountState::FieldStatusVerified(verified) => {
                let net_address = verified.net_address.clone();

                // Update single field.
                let notifications = self
                    .manager
                    .update_field(verified)
                    .unwrap()
                    .map(|changes| vec![changes.into()])
                    .unwrap_or(vec![]);

                let state = self.manager.lookup_full_state(&net_address).unwrap();

                (
                    net_address.clone(),
                    StateWrapper::with_notifications(state, notifications),
                )
            }
        };

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
        self.update_identity(msg);
    }
}
