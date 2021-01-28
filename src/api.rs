use crate::aggregate::request_handler::{
    RequestHandlerAggregate, RequestHandlerCommand, RequestHandlerId,
};
use crate::aggregate::EmptyStore;
use crate::event::{BlankNetwork, Event, StateWrapper};
use crate::state::{IdentityAddress, IdentityState, NetworkAddress};
use async_channel::{unbounded, Receiver, Sender};
use eventually::{Aggregate, Repository};
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
}

impl ConnectionPool {
    pub fn sender(&self, net_address: &NetworkAddress) -> Option<Sender<Event>> {
        self.pool
            .read()
            .get(net_address)
            .map(|info| info.sender.clone())
    }
    fn receiver(&self, net_address: &NetworkAddress) -> Receiver<Event> {
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
        _: Subscriber<StateWrapper>,
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
        subscriber: Subscriber<StateWrapper>,
        network: BlankNetwork,
        address: IdentityAddress,
    ) {
        let net_address = NetworkAddress::from(network, address);
        let receiver = self.connection_pool.receiver(&net_address);

        // Assign an ID to the subscriber.
        let sub_id: SubId = NumericIdProvider::new().next_id().into();
        let sink = match subscriber.assign_id(SubscriptionId::Number(sub_id.0)) {
            Ok(sink) => sink,
            Err(_) => {
                debug!("Connection has already been terminated");
                return;
            }
        };

        let identity_state = Arc::clone(&self.identity_state);
        let repository = Arc::clone(&self.repository);

        // Spawn notification handler.
        tokio::spawn(async move {
            // Check the cache on whether the identity is currently available.
            // If not, send a request and have the session handler take care of it.
            let info = identity_state.read().lookup_full_state(&net_address);
            if let Some(info) = info {
                if let Err(_) = sink.notify(Ok(info.into())) {
                    debug!("Closing connection");
                    return Ok(());
                }
            } else {
                let _ = repository
                    .get(RequestHandlerId)
                    .await?
                    .handle(RequestHandlerCommand::RequestState {
                        requester: sub_id,
                        net_address: net_address,
                    })
                    .await?;
            }

            while let Ok(event) = receiver.recv().await {
                //identity_state.upgradable_read()

                /*
                if let Err(_) = sink.notify(Ok(event.expe())) {
                    debug!("Closing connection");
                    break;
                }
                */
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
