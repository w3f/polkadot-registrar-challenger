use crate::state::IdentityAddress;
use futures::future;
use jsonrpc_core::{MetaIoHandler, Params, Result, Value};
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{typed::Subscriber, PubSubHandler, Session, SubscriptionId};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast::{self, Receiver, Sender};

pub struct ConnectionPool {
    pool: Arc<RwLock<HashMap<IdentityAddress, ConnectionInfo>>>,
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

impl ConnectionPool {
    fn acquire_sender(&self, net_address: &IdentityAddress) -> Sender<Params> {
        self.pool
            .read()
            .get(&net_address)
            .map(|info| info.sender.clone())
            .or_else(|| {
                let info = ConnectionInfo::new();
                let sender = info.sender.clone();
                self.pool.write().insert(net_address.clone(), info);
                Some(sender)
            })
            // Is always `Some(..)`
            .unwrap()
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
