use super::Result;
use crate::identity::{AccountState, CommsMessage, CommsVerifier, OnChainIdentity};
use crate::{Account, AccountType, PubKey};
use std::convert::{TryFrom, TryInto};
use tokio::time::{self, Duration};
use websockets::{Frame, WebSocket};

#[derive(Serialize, Deserialize)]
struct JudgementResponse {
    address: String,
    judgement: Judgement,
}

#[derive(Serialize, Deserialize)]
enum Judgement {
    #[serde(rename = "reasonable")]
    Reasonable,
    #[serde(rename = "erroneous")]
    Erroneous,
}

impl JudgementResponse {
    fn reasonable(address: String) -> Result<String> {
        Ok(serde_json::to_string(&JudgementResponse {
            address: address,
            judgement: Judgement::Reasonable,
        })?)
    }
    fn erroneous(address: String) -> Result<String> {
        Ok(serde_json::to_string(&JudgementResponse {
            address: address,
            judgement: Judgement::Erroneous,
        })?)
    }
}

#[derive(Serialize, Deserialize)]
struct JudgementRequest {
    address: String,
    accounts: Accounts,
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
    pub async fn start(mut self) {
        use CommsMessage::*;

        let mut interval = time::interval(Duration::from_millis(50));

        loop {
            match self.comms.try_recv() {
                Some(ValidAccount { context }) => {
                    self.client
                        .send_text(
                            JudgementResponse::reasonable("TODO".to_string()).unwrap(),
                            false,
                            true,
                        )
                        .await
                        .unwrap();
                }
                Some(InvalidAccount { context }) => {
                    self.client
                        .send_text(
                            JudgementResponse::erroneous("TODO".to_string()).unwrap(),
                            false,
                            true,
                        )
                        .await
                        .unwrap();
                }
                _ => {}
            }

            if self.client.ready_to_receive().unwrap() {
                match self.client.receive().await {
                    Ok(Frame::Text { payload, .. }) => {
                        self.comms.new_on_chain_identity(
                            &serde_json::from_str::<JudgementRequest>(&payload)
                                .unwrap()
                                .try_into()
                                .unwrap(),
                        );
                    }
                    _ => {}
                }
            }

            interval.tick().await;
        }
    }
}

impl TryFrom<JudgementRequest> for OnChainIdentity {
    type Error = failure::Error;

    fn try_from(request: JudgementRequest) -> Result<Self> {
        let accs = request.accounts;

        Ok(OnChainIdentity {
            pub_key: PubKey::try_from(request.address.as_bytes().to_vec())?,
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
        })
    }
}
