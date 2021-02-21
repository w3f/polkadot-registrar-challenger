use crate::event::{BlankNetwork, ErrorMessage, StateWrapper};
use crate::manager::{IdentityAddress, IdentityManager, NetworkAddress};
use futures::select_biased;
use futures::FutureExt;
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{
    manager::{IdProvider, NumericIdProvider},
    typed::Subscriber,
    Session, SubscriptionId,
};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
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
    active_sessions: Arc<RwLock<HashSet<SubscriptionId>>>,
}

impl PublicRpcApi {
    pub fn new(pool: ConnectionPool, manager: Arc<RwLock<IdentityManager>>) -> Self {
        PublicRpcApi {
            connection_pool: pool,
            manager: manager,
            active_sessions: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

impl PublicRpc for PublicRpcApi {
    type Metadata = Arc<Session>;

    fn subscribe_account_status(
        &self,
        session: Self::Metadata,
        subscriber: Subscriber<AccountStatusResponse>,
        network: BlankNetwork,
        address: IdentityAddress,
    ) {
        // Assign an ID to the subscriber.
        let sub_id = SubscriptionId::Number(NumericIdProvider::new().next_id());
        self.active_sessions.write().insert(sub_id.clone());
        let sink = match subscriber.assign_id(sub_id.clone()) {
            Ok(sink) => sink,
            Err(_) => {
                debug!("Connection has already been terminated");
                return;
            }
        };

        let net_address = NetworkAddress::from(network, address);
        let mut watcher = self.connection_pool.watch_net_address(&net_address);

        let manager = Arc::clone(&self.manager);
        let active_sessions = Arc::clone(&self.active_sessions);

        // Remove tracker of subscriber if session drops.
        let t_sessions = active_sessions.clone();
        let t_sub_id = sub_id.clone();
        session.on_drop(move || {
            t_sessions.write().remove(&t_sub_id);
        });

        // Spawn a task to handle notifications intended for all subscribers to
        // a specific topic, aka. state changes of a specific network address
        // (e.g. Polkadot address).
        //
        // Messages are generated in `crate::projection::identity_change_notifier`.
        // TODO: Handle zombie threads.
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

                let mut recv = watcher.recv().boxed().fuse();
                let mut session_active = async {
                    tokio_02::time::delay_for(Duration::from_secs(1)).await;
                    active_sessions.read().contains(&sub_id)
                }.boxed().fuse();

                // Start event loop and keep the subscriber informed about any state changes.
                loop {
                    select_biased! {
                        current_state = recv => {
                            if let Some(current_state) = current_state {
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
                        },
                        is_active = session_active => {
                            if !is_active {
                                debug!("Ending thread for subscription ID: {:?}", sub_id);
                                break;
                            }
                        }
                    };
                }

                crate::Result::<()>::Ok(())
            })
            .unwrap();
        });
    }
    fn unsubscribe_account_status(
        &self,
        _: Option<Self::Metadata>,
        id: SubscriptionId,
    ) -> Result<bool> {
        self.active_sessions.write().remove(&id);
        Ok(true)
    }
}
