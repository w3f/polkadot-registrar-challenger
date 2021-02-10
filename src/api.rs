use crate::event::{BlankNetwork, ErrorMessage, Event, EventType, StateWrapper};
use crate::manager::{IdentityAddress, IdentityManager, NetworkAddress};
use crate::{
    aggregate::verifier::{VerifierAggregate, VerifierAggregateId},
    system::run_api_service,
};
use async_channel::{unbounded, Receiver, Sender};
use eventually::Repository;
use eventually_event_store_db::EventStore;
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;
use jsonrpc_pubsub::{
    manager::{IdProvider, NumericIdProvider},
    typed::Subscriber,
    Session, SubscriptionId,
};
use parking_lot::RwLock;
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

#[derive(Default)]
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

#[derive(Default)]
pub struct PublicRpcApi {
    connection_pool: ConnectionPool,
    manager: Arc<RwLock<IdentityManager>>,
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
        let watcher = self.connection_pool.watch_net_address(&net_address);

        let manager = Arc::clone(&self.manager);

        // Spawn a task to handle notifications intended for all subscribers to
        // a specific topic, aka. state changes of a specific network address
        // (e.g. Polkadot address).
        //
        // Messages are generated in `crate::projection::identity_change_notifier`.
        tokio::spawn(async move {
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
                if let Err(_) = sink.notify(Ok(AccountStatusResponse::Err(ErrorMessage {
                    code: NO_PENDING_JUDGMENT_REQUEST_CODE,
                    message: "There is not pending judgement for this identity (registrar #0)"
                        .to_string(),
                }))) {
                    debug!("Connection closed");
                    return Ok(());
                }
            }

            // Start event loop and keep the subscriber informed about any state changes.
            while let Ok(event) = watcher.recv().await {
                match event.body {
                    EventType::FieldStatusVerified(verified) => {
                        let net_address = &verified.net_address.clone();

                        // Update fields and get notifications (if any)
                        let notifications = match manager.write().update_field(verified) {
                            Ok(try_changes) => try_changes
                                .map(|changes| vec![changes.into()])
                                .unwrap_or(vec![]),
                            Err(_) => {
                                error!(
                                    "Failed to update field: identity {:?} does not exit",
                                    net_address
                                );

                                continue;
                            }
                        };

                        // Finally, fetch the current state and send it to the subscriber.
                        match manager.read().lookup_full_state(&net_address) {
                            Some(identity_state) => {
                                if let Err(_) = sink.notify(Ok(AccountStatusResponse::Ok(
                                    StateWrapper::with_notifications(identity_state, notifications),
                                ))) {
                                    debug!("Connection closed");
                                    return Ok(());
                                }
                            }
                            None => error!("Identity state not found in cache"),
                        }
                    }
                    EventType::IdentityInserted(inserted) => {
                        manager.write().insert_identity(inserted.clone());

                        if let Err(_) = sink.notify(Ok(AccountStatusResponse::Ok(
                            StateWrapper::from(inserted.identity),
                        ))) {
                            debug!("Connection closed");
                            return Ok(());
                        }
                    }
                    _ => {
                        warn!("Received unexpected event")
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
