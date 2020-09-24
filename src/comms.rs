use crate::identity::{AccountStatus, OnChainIdentity};
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, Fatal, Judgement, NetAccount, NetworkAddress,
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
    JudgeIdentity {
        network_address: NetworkAddress,
        judgement: Judgement,
    },
    LeaveRoom {
        room_id: RoomId,
    },
    AccountToVerify {
        net_account: NetAccount,
        account: Account,
    }
}

#[derive(Debug, Clone)]
pub struct CommsMain {
    sender: Sender<CommsMessage>,
}

impl CommsMain {

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
    pub fn notify_new_identity(&self, ident: OnChainIdentity) {
        self.tx
            .send(CommsMessage::NewJudgementRequest(ident))
            .fatal();
    }
}
