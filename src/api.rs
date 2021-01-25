use crate::state::NetworkAddress;
use futures::future;
use jsonrpc_core::{MetaIoHandler, Params, Result, Value};
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{typed::Subscriber, PubSubHandler, Session, SubscriptionId};
use lock_api::RwLockReadGuard;
use parking_lot::{RawRwLock, RwLock};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast::{self, Receiver, Sender};

pub struct ConnectionPool {
    pool: Arc<RwLock<HashMap<NetworkAddress, ConnectionInfo>>>,
}

pub struct ConnectionPoolLock<'a> {
    lock: RwLockReadGuard<'a, RawRwLock, HashMap<NetworkAddress, ConnectionInfo>>,
}

impl<'a> ConnectionPoolLock<'a> {
    pub fn sender(&'a self, net_address: &NetworkAddress) -> Option<&'a Sender<Params>> {
        self.lock.get(net_address).map(|info| &info.sender)
    }
}

impl ConnectionPool {
    fn new() -> Self {
        ConnectionPool {
            pool: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub fn acquire_lock<'a>(&'a self) -> ConnectionPoolLock<'a> {
        ConnectionPoolLock {
            lock: self.pool.read(),
        }
    }
}

struct ConnectionInfo {
    sender: Sender<Params>,
    receiver: Receiver<Params>,
}

impl ConnectionInfo {
    fn new() -> Self {
        let (sender, receiver) = broadcast::channel(1_000);

        ConnectionInfo {
            sender: sender,
            receiver: receiver,
        }
    }
}

#[rpc]
pub trait Rpc {
    type Metadata;

    #[pubsub(
        subscription = "account_status",
        subscribe,
        name = "account_subscribeStatus"
    )]
    fn subscribe_account_status(&self, _: Self::Metadata, _: Subscriber<String>, param: u64);
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
