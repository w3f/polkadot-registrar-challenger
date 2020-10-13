use crate::manager::OnChainIdentity;
use crate::primitives::{Account, AccountType, Fatal, Judgement, NetAccount};
use crossbeam::channel::{unbounded, Receiver, Sender};
use tokio::time::{self, Duration};

pub fn generate_comms(
    sender: Sender<CommsMessage>,
    account_ty: AccountType,
) -> (CommsMain, CommsVerifier) {
    let (tx, recv) = unbounded();

    (
        CommsMain { sender: tx },
        CommsVerifier {
            sender: sender,
            recv: recv,
            address_ty: account_ty,
        },
    )
}

pub enum CommsMessage {
    NewJudgementRequest(OnChainIdentity),
    JudgeIdentity {
        net_account: NetAccount,
        judgement: Judgement,
    },
    LeaveRoom {
        net_account: NetAccount,
    },
    AccountToVerify {
        net_account: NetAccount,
        account: Account,
    },
    NotifyStatusChange {
        net_account: NetAccount,
    },
    MessageAcknowledged,
    InvalidDisplayName {
        net_account: NetAccount,
        violations: Vec<Account>,
    }
}

#[derive(Debug, Clone)]
pub struct CommsMain {
    sender: Sender<CommsMessage>,
}

impl CommsMain {
    pub fn notify_account_verification(&self, net_account: NetAccount, account: Account) {
        self.sender
            .send(CommsMessage::AccountToVerify {
                net_account: net_account,
                account: account,
            })
            .fatal();
    }
    pub fn notify_identity_judgment(&self, net_account: NetAccount, judgment: Judgement) {
        self.sender
            .send(CommsMessage::JudgeIdentity {
                net_account: net_account,
                judgement: judgment,
            })
            .fatal();
    }
    pub fn leave_matrix_room(&self, net_account: NetAccount) {
        self.sender
            .send(CommsMessage::LeaveRoom {
                net_account: net_account,
            })
            .fatal();
    }
}

#[derive(Debug, Clone)]
pub struct CommsVerifier {
    sender: Sender<CommsMessage>,
    recv: Receiver<CommsMessage>,
    address_ty: AccountType,
}

impl CommsVerifier {
    #[cfg(test)]
    /// Create a CommsVerifier without any useful functionality. Only used as a
    /// filler for certain tests.
    pub fn new() -> Self {
        let (tx, recv) = unbounded();

        CommsVerifier {
            sender: tx,
            recv: recv,
            address_ty: AccountType::Matrix,
        }
    }
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
        self.sender
            .send(CommsMessage::NewJudgementRequest(ident))
            .fatal();
    }
    pub fn notify_status_change(&self, net_account: NetAccount) {
        self.sender
            .send(CommsMessage::NotifyStatusChange {
                net_account: net_account,
            })
            .fatal()
    }
    pub fn notify_invalid_display_name(&self, net_account: NetAccount, violations: Vec<Account>) {
        self.sender
            .send(CommsMessage::InvalidDisplayName {
                net_account: net_account,
                violations: violations,
            })
            .fatal()
    }
    pub fn notify_ack(&self) {
        self.sender.send(CommsMessage::MessageAcknowledged).fatal();
    }
}
