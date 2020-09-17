use crate::comms::{CommsMessage, CommsVerifier};
use crate::identity::{AccountState, OnChainIdentity};
use crate::primitives::{Account, AccountType, NetAccount, NetworkAddress, Result};
use serde_json::Value;
use std::convert::{TryFrom, TryInto};
use std::result::Result as StdResult;
use tokio::time::{self, Duration};
use websockets::{Frame, WebSocket};

#[derive(Serialize, Deserialize)]
enum EventType {
    #[serde(rename = "ack")]
    Ack,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "newJudgementRequest")]
    NewJudgementRequest,
    #[serde(rename = "judgementResult")]
    JudgementResult,
}

#[derive(Serialize, Deserialize)]
struct Message {
    event: EventType,
    data: Value,
}

#[derive(Serialize, Deserialize)]
struct AckResponse {
    result: String,
}

#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

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

#[derive(Debug, Fail)]
enum ConnectorError {
    #[fail(display = "")]
    InvalidMessage,
    #[fail(display = "")]
    Response,
}

impl Connector {
    pub async fn new(url: &str, comms: CommsVerifier) -> Result<Self> {
        Ok(Connector {
            client: WebSocket::connect(url).await?,
            comms: comms,
        })
    }
    pub async fn start(mut self) {
        use EventType::*;

        loop {
            let _ = self.local().await.map_err(|err| {
                // TODO: Log
                ()
            });
        }
    }
    async fn send_ack(&mut self) -> StdResult<(), ConnectorError> {
        self.client
            .send_text(
                serde_json::to_string(&Message {
                    event: EventType::Ack,
                    data: serde_json::to_value(&AckResponse {
                        result: "TODO".to_string(),
                    })
                    .map_err(|_| ConnectorError::Response)?,
                })
                .map_err(|_| ConnectorError::Response)?,
                false,
                true,
            )
            .await
            .map_err(|_| ConnectorError::Response)
            .map(|_| ())
    }
    async fn send_error(&mut self) -> StdResult<(), ConnectorError> {
        self.client
            .send_text(
                serde_json::to_string(&Message {
                    event: EventType::Error,
                    data: serde_json::to_value(&ErrorResponse {
                        error: "TODO".to_string(),
                    })
                    .map_err(|_| ConnectorError::Response)?,
                })
                .map_err(|_| ConnectorError::Response)?,
                false,
                true,
            )
            .await
            .map_err(|_| ConnectorError::Response)
            .map(|_| ())
    }
    async fn local(&mut self) -> StdResult<(), ConnectorError> {
        match self.client.receive().await {
            Ok(Frame::Text { payload, .. }) => {
                let msg = serde_json::from_str::<Message>(&payload).unwrap();

                match msg.event {
                    NewJudgementRequest => {
                        if let Ok(request) = serde_json::from_value::<JudgementRequest>(msg.data) {
                            if let Ok(ident) = OnChainIdentity::try_from(request) {
                                self.send_ack().await?;
                            } else {
                                self.send_error().await?;
                            }
                        } else {
                            self.send_error().await?;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
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
