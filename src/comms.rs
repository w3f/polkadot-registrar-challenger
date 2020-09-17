use crate::identity::{AccountStatus, OnChainIdentity};
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, Fatal, NetAccount, NetworkAddress,
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
    NewJudgementRequest(OnChainIdentity),
    VerifyAccount {
        network_address: NetworkAddress,
        account: Account,
        challenge: Challenge,
        room_id: Option<RoomId>,
    },
    UpdateAccountStatus {
        network_address: NetworkAddress,
        account_ty: AccountType,
        account_validity: AccountStatus,
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
    UpdateChallengeStatus {
        network_address: NetworkAddress,
        account_ty: AccountType,
        status: ChallengeStatus,
    },
    JudgeIdentity {
        network_address: NetworkAddress,
    },
}

#[derive(Debug, Clone)]
pub struct CommsMain {
    sender: Sender<CommsMessage>,
}

impl CommsMain {
    pub fn notify_account_verification(
        &self,
        network_address: NetworkAddress,
        account: Account,
        challenge: Challenge,
        room_id: Option<RoomId>,
    ) {
        self.sender
            .send(CommsMessage::VerifyAccount {
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
    pub fn judge_identity(&self, network_address: NetworkAddress) {
        self.sender
            .send(CommsMessage::JudgeIdentity {
                network_address: network_address,
            })
            .fatal();
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
        // No async support for `recv` (it blocks and chokes tokio), so we
        // `try_recv` and just loop over it with a short pause.
        let mut interval = time::interval(Duration::from_millis(10));
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
    /// Receive a `VerifyAccount` message. This is only used by the Matrix client as
    /// any other message type will panic.
    pub async fn recv_inform(&self) -> (NetworkAddress, Account, Challenge, Option<RoomId>) {
        if let CommsMessage::VerifyAccount {
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
    pub fn notify_account_state_request(&self, account: Account, account_ty: AccountType) {
        self.tx
            .send(CommsMessage::RequestAccountState {
                account: account,
                account_ty: account_ty,
            })
            .fatal();
    }
    pub fn notify_new_identity(&self, ident: OnChainIdentity) {
        self.tx
            .send(CommsMessage::NewJudgementRequest(ident))
            .fatal();
    }
    pub fn notify_account_status(
        &self,
        network_address: NetworkAddress,
        account_ty: AccountType,
        account_validity: AccountStatus,
    ) {
        self.tx
            .send(CommsMessage::UpdateAccountStatus {
                network_address: network_address,
                account_ty: account_ty,
                account_validity: account_validity,
            })
            .fatal();
    }
    pub fn notify_room_id(&self, address: NetAccount, room_id: RoomId) {
        self.tx
            .send(CommsMessage::TrackRoomId {
                address: address,
                room_id: room_id,
            })
            .fatal();
    }
    pub fn notify_challenge_status(
        &self,
        network_address: NetworkAddress,
        account_ty: AccountType,
        status: ChallengeStatus,
    ) {
        self.tx
            .send(CommsMessage::UpdateChallengeStatus {
                network_address: network_address,
                account_ty: account_ty,
                status,
            })
            .fatal()
    }
}
