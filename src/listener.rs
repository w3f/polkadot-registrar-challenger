use super::Result;
use crate::identity::{AccountState, CommsMessage, CommsVerifier, OnChainIdentity};
use crate::{Account, AccountType, PubKey};
use std::convert::TryFrom;
use websockets::WebSocket;

#[derive(Serialize, Deserialize)]
pub struct JudgementResponse {
    pub address: String,
    pub judgement: String,
}

#[derive(Serialize, Deserialize)]
pub struct JudgementRequest {
    pub address: String,
    pub accounts: Accounts,
}

#[derive(Serialize, Deserialize)]
pub struct Accounts {
    display_name: Option<String>,
    legal_name: Option<String>,
    email: Option<String>,
    web: Option<String>,
    twitter: Option<String>,
    matrix: Option<String>,
}

pub struct Listener {
    client: WebSocket,
    comms: CommsVerifier,
}

impl Listener {
    pub async fn new(url: &str, comms: CommsVerifier) -> Result<Self> {
        Ok(Listener {
            client: WebSocket::connect(url).await?,
            comms: comms,
        })
    }
    pub async fn start(self) {
        use CommsMessage::*;

        match self.comms.recv().await {
            ValidAccount { context } => {}
            InvalidAccount { context } => {}
            _ => {}
        }
    }
}

impl From<JudgementRequest> for OnChainIdentity {
    fn from(request: JudgementRequest) -> Self {
        let accs = request.accounts;

        OnChainIdentity {
            pub_key: PubKey::try_from(vec![]).unwrap(),
            display_name: accs.display_name,
            legal_name: accs.legal_name,
            email: accs
                .email
                .map(|v| AccountState::new(Account::from(v), AccountType::Email)),
            web: accs
                .web
                .map(|v| AccountState::new(Account::from(v), AccountType::Web)),
            twitter: accs
                .twitter
                .map(|v| AccountState::new(Account::from(v), AccountType::Twitter)),
            matrix: accs
                .matrix
                .map(|v| AccountState::new(Account::from(v), AccountType::Matrix)),
        }
    }
}
