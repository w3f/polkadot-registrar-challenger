use crate::event::{BlankNetwork, Event, StateWrapper};
use crate::state::{IdentityAddress, NetworkAddress};
use async_channel::{unbounded, Receiver, Sender};
use futures::future;
use futures::StreamExt;
use jsonrpc_core::{MetaIoHandler, Params, Result, Value};
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{typed::Subscriber, PubSubHandler, Session, SubscriptionId};
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
    pub fn receiver(&self, net_address: &NetworkAddress) -> Option<Receiver<Event<StateWrapper>>> {
        self.pool
            .read()
            .get(net_address)
            .map(|info| info.receiver.clone())
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
        _: Subscriber<String>,
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
        _: Subscriber<String>,
        network: BlankNetwork,
        address: IdentityAddress,
    ) {
        let net_address = NetworkAddress::from(network, address);
        let receiver = self.connection_pool.receiver(&net_address).unwrap();
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
