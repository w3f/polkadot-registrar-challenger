use crate::aggregate::request_handler::{
    RequestHandlerAggregate, RequestHandlerCommand, RequestHandlerId,
};
use crate::aggregate::EmptyStore;
use crate::event::{
    BlankNetwork, ErrorMessage, Event, EventType, FieldStatusVerified, StateWrapper,
};
use crate::state::{IdentityAddress, IdentityInfo, IdentityState, NetworkAddress};
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

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct SubId(u64);

impl From<u64> for SubId {
    fn from(val: u64) -> Self {
        SubId(val)
    }
}

pub struct ConnectionPool {
    // TODO: Arc/RwLock around HashMap necessary?
    pool: Arc<RwLock<HashMap<NetworkAddress, ConnectionInfo>>>,
    direct: Arc<RwLock<HashMap<SubId, ConnectionInfo>>>,
}

impl ConnectionPool {
    pub fn notify_net_address(&self, net_address: &NetworkAddress) -> Option<Sender<Event>> {
        self.pool
            .read()
            .get(net_address)
            .map(|info| info.sender.clone())
    }
    fn watch_net_address(&self, net_address: &NetworkAddress) -> Receiver<Event> {
        self.pool
            .read()
            .get(net_address)
            .map(|info| info.receiver.clone())
            .or_else(|| {
                let info = ConnectionInfo::new();
                let receiver = info.receiver.clone();
                self.pool.write().insert(net_address.clone(), info);
                Some(receiver)
            })
            // Always returns `Some(...)`.
            .unwrap()
    }
    fn register_direct(&self, sub_id: SubId) -> Receiver<Event> {
        self.direct
            .write()
            .entry(sub_id)
            .or_insert(ConnectionInfo::new())
            .receiver
            .clone()
    }
    fn get_direct(&self, sub_id: &SubId) -> Option<Sender<Event>> {
        self.direct
            .read()
            .get(sub_id)
            .map(|info| info.sender.clone())
    }
}

impl ConnectionPool {
    fn new() -> Self {
        ConnectionPool {
            pool: Arc::new(RwLock::new(HashMap::new())),
            direct: Arc::new(RwLock::new(HashMap::new())),
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

type AccountStatusResponse = RpcResponse<AccountStatusMessage, ErrorMessage>;

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AccountStatusMessage {
    StateWrapper(StateWrapper),
    IdentityInfo(IdentityInfo),
}

impl From<StateWrapper> for AccountStatusMessage {
    fn from(val: StateWrapper) -> Self {
        AccountStatusMessage::StateWrapper(val)
    }
}

impl From<IdentityInfo> for AccountStatusMessage {
    fn from(val: IdentityInfo) -> Self {
        AccountStatusMessage::IdentityInfo(val)
    }
}

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
    identity_state: Arc<RwLock<IdentityState<'static>>>,
    repository: Arc<
        Repository<
            RequestHandlerAggregate,
            EmptyStore<
                <RequestHandlerAggregate as Aggregate>::Id,
                <RequestHandlerAggregate as Aggregate>::Event,
            >,
        >,
    >,
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
        let direct_listener = self.connection_pool.register_direct(sub_id.clone());

        let identity_state = Arc::clone(&self.identity_state);
        let repository = Arc::clone(&self.repository);

        // Spawn a task to handle direct messages intended for the individual subscriber.
        //
        // Messages are generated in `crate::projection::response_notifier`.
        let sink_direct = sink.clone();
        tokio::spawn(async move {
            while let Ok(event) = direct_listener.recv().await {
                // Determine whether the event is a regular message or an error message.
                let response = match event.body {
                    EventType::IdentityInfo(val) => AccountStatusResponse::Ok(val.into()),
                    EventType::ErrorMessage(val) => AccountStatusResponse::Err(val.into()),
                    _ => {
                        error!("Received unrecognized event from direct line");
                        continue;
                    }
                };

                // Send message to subscriber.
                if let Err(_) = sink_direct.notify(Ok(response)) {
                    debug!("Connection closed");
                    return;
                }
            }
        });

        // Spawn a task to handle notifications intended for all subscribers to
        // a specific topic, aka. state changes of a specific network address
        // (e.g. Polkadot address).
        //
        // Messages are generated in `crate::projection::state_change_notifier`.
        tokio::spawn(async move {
            // Check the cache on whether the identity is currently available...
            let info = identity_state.read().lookup_full_state(&net_address);
            if let Some(info) = info {
                if let Err(_) = sink.notify(Ok(AccountStatusResponse::Ok(info.into()))) {
                    debug!("Connection closed");
                    return Ok(());
                }
            } else {
                // ... if not, send a request and have the "direct message" handler take care of it.
                let _ = repository
                    .get(RequestHandlerId)
                    .await?
                    .handle(RequestHandlerCommand::RequestState {
                        requester: sub_id,
                        net_address: net_address,
                    })
                    .await?;
            }

            // Queue for state changes.
            let mut changes_queue: Vec<FieldStatusVerified> = vec![];
            while let Ok(event) = watcher.recv().await {
                match event.body {
                    EventType::FieldStatusVerified(field_changes_verified) => {
                        let net_address = &field_changes_verified.net_address;

                        // Check if the identity exists in cache. If not, queue
                        // the change and apply it later on a `EventType::IdentityInfo`
                        // event. This behavior could occur if changes are
                        // received before the requested state.
                        if !identity_state.read().exists(net_address) {
                            changes_queue.push(field_changes_verified);

                            continue;
                        }

                        let field_status = field_changes_verified.field_status;

                        // Update the identity with the state change.
                        if let Err(err) = identity_state
                            .write()
                            .update_field(&net_address, field_status)
                        {
                            error!("{}", err);
                            continue;
                        };

                        // Finally, fetch the current state and send it to the subscriber.
                        match identity_state.read().lookup_full_state(&net_address) {
                            Some(full_state) => {
                                if let Err(_) =
                                    sink.notify(Ok(AccountStatusResponse::Ok(full_state.into())))
                                {
                                    debug!("Connection closed");
                                    return Ok(());
                                }
                            }
                            None => error!("Identity state not found in cache"),
                        }
                    }
                    EventType::IdentityInfo(full_state) => {
                        // Apply all queued changes to the identity state.
                        for change in changes_queue {
                            let (net_address, field_status) =
                                (change.net_address, change.field_status);
                            if let Err(err) = identity_state
                                .write()
                                .update_field(&net_address, field_status)
                            {
                                error!("{}", err);
                                continue;
                            }
                        }

                        // Wipe the changes queue (reinitialize to get around
                        // the borrow-checker).
                        changes_queue = vec![];

                        // Send current state to the subscriber.
                        match identity_state
                            .read()
                            .lookup_full_state(&full_state.net_address)
                        {
                            Some(full_state) => {
                                if let Err(_) =
                                    sink.notify(Ok(AccountStatusResponse::Ok(full_state.into())))
                                {
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
