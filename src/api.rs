use crate::event::{BlankNetwork, ErrorMessage, Event, EventType, StateWrapper};
use crate::manager::{IdentityAddress, IdentityManager, NetworkAddress};
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{
    manager::{IdProvider, NumericIdProvider},
    typed::Subscriber,
    Session, SubscriptionId,
};
use parking_lot::{RwLock, RwLockReadGuard};
use std::collections::HashMap;
use std::sync::Arc;
use tokio_02::sync::watch::{channel, Receiver, Sender};

const REGISTRAR_IDX: usize = 0;

// TODO: Remove this type since it is no longer necessary.
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct SubId(u64);

impl From<u64> for SubId {
    fn from(val: u64) -> Self {
        SubId(val)
    }
}

#[derive(Default, Clone)]
pub struct ConnectionPool {
    pool: Arc<RwLock<HashMap<NetworkAddress, ConnectionInfo>>>,
}

impl ConnectionPool {
    pub fn broadcast(&self, net_address: &NetworkAddress, state: StateWrapper) {
        self.pool
            .read()
            .get(net_address)
            .map(|info| info.sender.broadcast(Some(state)));
    }
    fn watch_net_address(&self, net_address: &NetworkAddress) -> Receiver<Option<StateWrapper>> {
        let mut writer = self.pool.write();
        if let Some(info) = writer.get(net_address) {
            info.receiver.clone()
        } else {
            let info = ConnectionInfo::new();
            let receiver = info.receiver.clone();
            writer.insert(net_address.clone(), info);
            receiver
        }
    }
}

struct ConnectionInfo {
    sender: Sender<Option<StateWrapper>>,
    receiver: Receiver<Option<StateWrapper>>,
}

impl ConnectionInfo {
    fn new() -> Self {
        let (sender, receiver) = channel(None);

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

pub struct PublicRpcApi {
    connection_pool: ConnectionPool,
    manager: Arc<RwLock<IdentityManager>>,
}

impl PublicRpcApi {
    pub fn new(pool: ConnectionPool, manager: Arc<RwLock<IdentityManager>>) -> Self {
        PublicRpcApi {
            connection_pool: pool,
            manager: manager,
        }
    }
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
        let sub_id = NumericIdProvider::new().next_id();
        let sink = match subscriber.assign_id(SubscriptionId::Number(sub_id)) {
            Ok(sink) => sink,
            Err(_) => {
                debug!("Connection has already been terminated");
                return;
            }
        };

        let net_address = NetworkAddress::from(network, address);
        let mut watcher = self.connection_pool.watch_net_address(&net_address);

        let manager = Arc::clone(&self.manager);

        // Spawn a task to handle notifications intended for all subscribers to
        // a specific topic, aka. state changes of a specific network address
        // (e.g. Polkadot address).
        //
        // Messages are generated in `crate::projection::identity_change_notifier`.
        std::thread::spawn(move || {
            let mut rt = tokio_02::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                // Check the state for the requested identity and respond
                // immediately with the current state.
                let try_state = manager.read().lookup_full_state(&net_address);
                if let Some(state) = try_state {
                    // Send the current state to the subscriber.
                    if let Err(_) = sink.notify(Ok(AccountStatusResponse::Ok(state.into()))) {
                        debug!("Connection closed");
                        return Ok(());
                    }
                } else {
                    if let Err(_) = sink.notify(Ok(AccountStatusResponse::Err(
                        ErrorMessage::no_pending_judgement_request(REGISTRAR_IDX),
                    ))) {
                        debug!("Connection closed");
                        return Ok(());
                    }
                }

                // Start event loop and keep the subscriber informed about any state changes.
                while let Some(current_state) = watcher.recv().await {
                    let current_state = match current_state {
                        Some(current_state) => current_state,
                        None => continue,
                    };

                    // Notify client.
                    if let Err(_) = sink.notify(Ok(AccountStatusResponse::Ok(current_state))) {
                        debug!("Connection closed");
                        return Ok(());
                    }
                }

                crate::Result::<()>::Ok(())
            })
            .unwrap();
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
