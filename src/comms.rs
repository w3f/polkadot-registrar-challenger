use crate::identity::OnChainIdentity;
use crate::primitives::{
    Account, AccountType, Challenge, Fatal, NetAccount, NetworkAddress, PubKey,
};
use crossbeam::channel::{unbounded, Receiver, Sender};

use matrix_sdk::identifiers::RoomId;

use tokio::time::{self, Duration};

pub fn generate_comms(
    listener: Sender<CommsMessage>,
    account_ty: AccountType,
) -> (CommsMain, CommsVerifier) {
    let (tx, recv) = unbounded();

    (
        CommsMain { sender: tx },
        CommsVerifier {
            tx: listener,
            recv: recv,
            address_ty: account_ty,
        },
    )
}

pub enum CommsMessage {
    NewOnChainIdentity(OnChainIdentity),
    Inform {
        network_address: NetworkAddress,
        account: Account,
        challenge: Challenge,
        room_id: Option<RoomId>,
    },
    ValidAccount {
        network_address: NetworkAddress,
    },
    InvalidAccount {
        network_address: NetworkAddress,
    },
    TrackRoomId {
        address: NetAccount,
        room_id: RoomId,
    },
    RequestAccountState {
        account: Account,
        account_ty: AccountType,
    },
    InvalidRequest,
}

#[derive(Debug, Clone)]
pub struct CommsMain {
    sender: Sender<CommsMessage>,
}

impl CommsMain {
    pub fn inform(
        &self,
        network_address: NetworkAddress,
        account: Account,
        challenge: Challenge,
        room_id: Option<RoomId>,
    ) {
        println!("INFORMING");
        self.sender
            .send(CommsMessage::Inform {
                network_address: network_address,
                account: account,
                challenge: challenge,
                room_id,
            })
            .fatal();
    }
    pub fn invalid_request(&self) {
        self.sender.send(CommsMessage::InvalidRequest).fatal();
    }
}

#[derive(Debug, Clone)]
pub struct CommsVerifier {
    tx: Sender<CommsMessage>,
    recv: Receiver<CommsMessage>,
    address_ty: AccountType,
}

impl CommsVerifier {
    pub async fn recv(&self) -> CommsMessage {
        let mut interval = time::interval(Duration::from_millis(50));

        loop {
            if let Ok(msg) = self.recv.try_recv() {
                return msg;
            } else {
                interval.tick().await;
            }
        }
    }
    pub fn try_recv(&self) -> Option<CommsMessage> {
        self.recv.try_recv().ok()
    }
    /// Receive a `Inform` message. This is only used by the Matrix client as
    /// any other message type will panic.
    // TODO: Just use `recv` and match directly. Remove this method
    pub async fn recv_inform(&self) -> (NetworkAddress, Account, Challenge, Option<RoomId>) {
        if let CommsMessage::Inform {
            network_address,
            account,
            challenge,
            room_id,
        } = self.recv().await
        {
            (network_address, account, challenge, room_id)
        } else {
            panic!("received invalid message type on Matrix client");
        }
    }
    pub fn request_account_state(&self, account: Account, account_ty: AccountType) {
        self.tx
            .send(CommsMessage::RequestAccountState {
                account: account,
                account_ty: account_ty,
            })
            .fatal();
    }
    pub fn new_on_chain_identity(&self, ident: OnChainIdentity) {
        self.tx
            .send(CommsMessage::NewOnChainIdentity(ident))
            .fatal();
    }
    pub fn valid_feedback(&self, network_address: NetworkAddress) {
        self.tx
            .send(CommsMessage::ValidAccount {
                network_address: network_address,
            })
            .fatal();
    }
    pub fn invalid_feedback(&self, network_address: NetworkAddress) {
        self.tx
            .send(CommsMessage::InvalidAccount {
                network_address: network_address,
            })
            .fatal();
    }
    pub fn track_room_id(&self, address: NetAccount, room_id: RoomId) {
        self.tx
            .send(CommsMessage::TrackRoomId {
                address: address,
                room_id: room_id,
            })
            .fatal();
    }
}
