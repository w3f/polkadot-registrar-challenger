use crate::manager::OnChainIdentity;
use crate::primitives::{Account, AccountType, Fatal, Judgement, NetAccount};
#[cfg(test)]
use crate::tests::mocks::MatrixEventMock;
use crossbeam::channel::{unbounded, Receiver, Sender};
#[cfg(test)]
use matrix_sdk::identifiers::{RoomId, UserId};
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
    NotifyInvalidAccount {
        net_account: NetAccount,
        account: Account,
        accounts: Vec<(AccountType, Account)>,
    },
    ExistingDisplayNames {
        accounts: Vec<Account>,
    },
    JudgementGivenAck {
        net_account: NetAccount,
    },
    // Only used to manually trigger the event handler in tests, since the
    // matrix sdk runs the EventEmitter in the background.
    #[cfg(test)]
    TriggerMatrixEmitter {
        room_id: RoomId,
        my_user_id: UserId,
        event: MatrixEventMock,
    },
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
    pub fn notify_invalid_accounts(
        &self,
        net_account: NetAccount,
        account: Account,
        accounts: Vec<(AccountType, Account)>,
    ) {
        self.sender
            .send(CommsMessage::NotifyInvalidAccount {
                net_account: net_account,
                account: account,
                accounts: accounts,
            })
            .fatal()
    }
    #[cfg(test)]
    pub fn trigger_matrix_emitter(
        &self,
        room_id: RoomId,
        my_user_id: UserId,
        event: MatrixEventMock,
    ) {
        self.sender
            .send(CommsMessage::TriggerMatrixEmitter {
                room_id: room_id,
                my_user_id: my_user_id,
                event: event,
            })
            .fatal()
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
    pub fn notify_ack(&self) {
        self.sender.send(CommsMessage::MessageAcknowledged).fatal();
    }
    pub fn notify_judgement_given_ack(&self, net_account: NetAccount) {
        self.sender
            .send(CommsMessage::JudgementGivenAck {
                net_account: net_account,
            })
            .fatal()
    }
    pub fn notify_existing_display_names(&self, accounts: Vec<Account>) {
        self.sender
            .send(CommsMessage::ExistingDisplayNames { accounts: accounts })
            .fatal()
    }
}
