use crate::comms::{CommsMessage, CommsVerifier};
use crate::identity::{AccountState, OnChainIdentity};
use crate::primitives::{Account, AccountType, NetAccount, NetworkAddress, Result};
use std::convert::{TryFrom, TryInto};
use tokio::time::{self, Duration};
use websockets::{Frame, WebSocket};

#[derive(Serialize, Deserialize)]
struct JudgementResponse {
    address: NetAccount,
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
    fn reasonable(address: NetAccount) -> Result<String> {
        Ok(serde_json::to_string(&JudgementResponse {
            address: address,
            judgement: Judgement::Reasonable,
        })?)
    }
    fn erroneous(address: NetAccount) -> Result<String> {
        Ok(serde_json::to_string(&JudgementResponse {
            address: address,
            judgement: Judgement::Erroneous,
        })?)
    }
}

#[derive(Serialize, Deserialize)]
struct JudgementRequest {
    address: NetAccount,
    accounts: Accounts,
}

#[derive(Serialize, Deserialize)]
struct Accounts {
    display_name: Option<String>,
    legal_name: Option<String>,
    email: Option<Account>,
    web: Option<Account>,
    twitter: Option<Account>,
    matrix: Option<Account>,
}

pub struct Connector {
    client: WebSocket,
    comms: CommsVerifier,
}

impl Connector {
    pub async fn new(url: &str, comms: CommsVerifier) -> Result<Self> {
        Ok(Connector {
            client: WebSocket::connect(url).await?,
            comms: comms,
        })
    }
    pub async fn start(mut self) {
        use CommsMessage::*;

        let mut interval = time::interval(Duration::from_millis(50));

        loop {
            match self.comms.try_recv() {
                Some(ValidAccount { network_address }) => {
                    self.client
                        .send_text(
                            JudgementResponse::reasonable(network_address.address().clone())
                                .unwrap(),
                            false,
                            true,
                        )
                        .await
                        .unwrap();
                }
                Some(InvalidAccount { network_address }) => {
                    self.client
                        .send_text(
                            JudgementResponse::erroneous(network_address.address().clone())
                                .unwrap(),
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
                            serde_json::from_str::<JudgementRequest>(&payload)
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
            network_address: NetworkAddress::try_from(request.address)?,
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
