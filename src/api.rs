use crate::event::{
    BlankNetwork, ErrorMessage, Event, EventType, FieldStatusVerified, Notification, StateWrapper,
};
use crate::manager::{IdentityAddress, IdentityManager, IdentityState, NetworkAddress};
use async_channel::{unbounded, Receiver, Sender};
use eventually::{Aggregate, Repository};
use future::join;
use futures::StreamExt;
use futures::{future, join};
use jsonrpc_core::{MetaIoHandler, Params, Result, Value};
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{
    manager::{IdProvider, NumericIdProvider},
    typed::Subscriber,
    PubSubHandler, Session, SubscriptionId,
};
use lock_api::RwLockReadGuard;
use matrix_sdk::api::{
    error,
    r0::{receipt, sync::sync_events::State},
};
use parking_lot::{RawRwLock, RwLock};
use std::collections::HashMap;
use std::sync::Arc;

const REGISTRAR_ID: u32 = 0;
const NO_PENDING_JUDGMENT_REQUEST_CODE: u32 = 1000;

// TODO: Remove this type since it is no longer necessary.
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct SubId(u64);

impl From<u64> for SubId {
    fn from(val: u64) -> Self {
        SubId(val)
    }
}

pub struct ConnectionPool {
    pool: Arc<RwLock<HashMap<NetworkAddress, ConnectionInfo>>>,
}

impl ConnectionPool {
    pub fn notify_net_address(&self, net_address: &NetworkAddress) -> Option<Sender<Event>> {
        self.pool
            .read()
            .get(net_address)
            .map(|state| state.sender.clone())
    }
    fn watch_net_address(&self, net_address: &NetworkAddress) -> Receiver<Event> {
        self.pool
            .read()
            .get(net_address)
            .map(|state| state.receiver.clone())
            .or_else(|| {
                let state = ConnectionInfo::new();
                let receiver = state.receiver.clone();
                self.pool.write().insert(net_address.clone(), state);
                Some(receiver)
            })
            // Always returns `Some(...)`.
            .unwrap()
    }
}

impl ConnectionPool {
    fn new() -> Self {
        ConnectionPool {
            pool: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

struct ConnectionInfo {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

impl ConnectionInfo {
    fn new() -> Self {
        let (sender, receiver) = unbounded();

        ConnectionInfo {
            sender: sender,
            receiver: receiver,
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcResponse<T, E> {
    Ok(T),
    Err(E),
}

type AccountStatusResponse = RpcResponse<StateWrapper, ErrorMessage>;

#[rpc]
pub trait PublicRpc {
    type Metadata;

    #[pubsub(
        subscription = "account_status",
        subscribe,
        name = "account_subscribeStatus"
    )]
    fn subscribe_account_status(
        &self,
        _: Self::Metadata,
        _: Subscriber<AccountStatusResponse>,
        network: BlankNetwork,
        address: IdentityAddress,
    );
    #[pubsub(
        subscription = "account_status",
        unsubscribe,
        name = "account_unsubscribeStatus"
    )]
    fn unsubscribe_account_status(
        &self,
        _: Option<Self::Metadata>,
        _: SubscriptionId,
    ) -> Result<bool>;
}

struct PublicRpcApi {
    connection_pool: ConnectionPool,
    manager: Arc<RwLock<IdentityManager<'static>>>,
}

impl PublicRpc for PublicRpcApi {
    type Metadata = Arc<Session>;

    fn subscribe_account_status(
        &self,
        _: Self::Metadata,
        subscriber: Subscriber<AccountStatusResponse>,
        network: BlankNetwork,
        address: IdentityAddress,
    ) {
        // Assign an ID to the subscriber.
        let sub_id: SubId = NumericIdProvider::new().next_id().into();
        let sink = match subscriber.assign_id(SubscriptionId::Number(sub_id.0)) {
            Ok(sink) => sink,
            Err(_) => {
                debug!("Connection has already been terminated");
                return;
            }
        };

        let net_address = NetworkAddress::from(network, address);
        let watcher = self.connection_pool.watch_net_address(&net_address);

        let manager = Arc::clone(&self.manager);

        // Spawn a task to handle notifications intended for all subscribers to
        // a specific topic, aka. state changes of a specific network address
        // (e.g. Polkadot address).
        //
        // Messages are generated in `crate::projection::identity_change_notifier`.
        tokio::spawn(async move {
            // Check the state for the requested identity and respond
            // immediately with the current state. If the identity is not
            // available, it implies there is not pending judgement request (for
            // the specified registrar, at least).
            if let Some(state) = manager.read().lookup_full_state(&net_address) {
                if let Err(_) = sink.notify(Ok(AccountStatusResponse::Ok(state.into()))) {
                    debug!("Connection closed");
                    return Ok(());
                }
            } else {
                // ... if not, send a request and have the "direct message" handler take care of it.
                if let Err(_) = sink.notify(Ok(AccountStatusResponse::Err(ErrorMessage {
                    code: NO_PENDING_JUDGMENT_REQUEST_CODE,
                    message: format!(
                        "There is no pending judgement request for this identity (for registrar #{}",
                        REGISTRAR_ID
                    ),
                }))) {
                    debug!("Connection closed");
                    return Ok(());
                }
            }

            // Start event loop and keep the subscriber informed about any state changes.
            while let Ok(event) = watcher.recv().await {
                match event.body {
                    EventType::FieldStatusVerified(field_changes_verified) => {
                        let net_address = &field_changes_verified.net_address.clone();

                        // Update the identity with the state change and create notifications (if any).
                        let notification: Vec<Notification> =
                            match manager.write().update_field(field_changes_verified) {
                                Ok(try_notification) => try_notification
                                    .map(|changes| vec![changes.into()])
                                    .unwrap_or(vec![]),
                                Err(err) => {
                                    error!("{}", err);
                                    continue;
                                }
                            };

                        // Finally, fetch the current state and send it to the subscriber.
                        match manager.read().lookup_full_state(&net_address) {
                            Some(full_state) => {
                                if let Err(_) = sink.notify(Ok(AccountStatusResponse::Ok(
                                    StateWrapper::with_notifications(full_state, notification),
                                ))) {
                                    debug!("Connection closed");
                                    return Ok(());
                                }
                            }
                            None => error!("Identity state not found in cache"),
                        }
                    }
                    _ => {
                        error!("Received unexpected event. Ignoring.")
                    }
                }
            }

            crate::Result::<()>::Ok(())
        });
    }
    fn unsubscribe_account_status(
        &self,
        _: Option<Self::Metadata>,
        _: SubscriptionId,
    ) -> Result<bool> {
        Ok(true)
    }
}

pub fn start_api() {
    /*
    let mut io = PubSubHandler::new(MetaIoHandler::default());
    io.add_subscription(
        "account_status",
        (
            "account_subscribeStatus",
            move |params: Params, _: Arc<Session>, subscriber: Subscriber| {},
        ),
        ("account_unsubscribeStatus", move |id: SubscriptionId, _| {
            future::ok(Value::Null)
        }),
    );
    */
}
