use crate::event::{BlankNetwork, Event, StateWrapper};
use crate::state::{IdentityAddress, NetworkAddress};
use async_channel::{unbounded, Receiver, Sender};
use futures::future;
use futures::StreamExt;
use jsonrpc_core::{MetaIoHandler, Params, Result, Value};
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{
    manager::{IdProvider, NumericIdProvider},
    typed::Subscriber,
    PubSubHandler, Session, SubscriptionId,
};
use lock_api::RwLockReadGuard;
use matrix_sdk::api::r0::receipt;
use parking_lot::{RawRwLock, RwLock};
use std::collections::HashMap;
use std::sync::Arc;

pub struct ConnectionPool {
    // TODO: Arc/RwLock around HashMap necessary?
    pool: Arc<RwLock<HashMap<NetworkAddress, ConnectionInfo>>>,
}

impl ConnectionPool {
    pub fn sender(&self, net_address: &NetworkAddress) -> Option<Sender<Event<StateWrapper>>> {
        self.pool
            .read()
            .get(net_address)
            .map(|info| info.sender.clone())
    }
    // TODO: Unit test this!!
    fn receiver(&self, net_address: &NetworkAddress) -> Receiver<Event<StateWrapper>> {
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
}

impl ConnectionPool {
    fn new() -> Self {
        ConnectionPool {
            pool: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

struct ConnectionInfo {
    sender: Sender<Event<StateWrapper>>,
    receiver: Receiver<Event<StateWrapper>>,
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
        _: Subscriber<Event<StateWrapper>>,
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
}

impl PublicRpc for PublicRpcApi {
    type Metadata = Arc<Session>;

    fn subscribe_account_status(
        &self,
        _: Self::Metadata,
        subscriber: Subscriber<Event<StateWrapper>>,
        network: BlankNetwork,
        address: IdentityAddress,
    ) {
        let net_address = NetworkAddress::from(network, address);
        let receiver = self.connection_pool.receiver(&net_address);

        // Assign an ID to the subscriber.
        let sink = match subscriber
            .assign_id(SubscriptionId::Number(NumericIdProvider::new().next_id()))
        {
            Ok(sink) => sink,
            Err(_) => {
                debug!("Connection has already been terminated");
                return;
            }
        };

        // Spawn notification handler.
        tokio::spawn(async move {
            while let Ok(event) = receiver.recv().await {
                if let Err(_) = sink.notify(Ok(event)) {
                    debug!("Closing connection");
                    break;
                }
            }
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
