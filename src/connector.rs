use crate::comms::CommsVerifier;
use crate::identity::{AccountState, OnChainIdentity};
use crate::primitives::{Account, AccountType, NetAccount, NetworkAddress, Result};
use serde_json::Value;
use std::convert::TryFrom;
use std::result::Result as StdResult;
use websockets::{Frame, WebSocket};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    event: EventType,
    data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AckResponse {
    result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JudgementResponse {
    address: NetAccount,
    judgement: Judgement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JudgementRequest {
    address: NetAccount,
    accounts: Accounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let mut connector = Connector {
            client: WebSocket::connect(url).await?,
            comms: comms,
        };

        connector.send_ack(Some("Connection established")).await?;

        Ok(connector)
    }
    pub async fn start(mut self) {
        loop {
            let _ = self.local().await.map_err(|err| {
                // TODO: Log
                println!("Got error: {:?}", err);
                ()
            });
        }
    }
    async fn send_ack(&mut self, msg: Option<&str>) -> StdResult<(), ConnectorError> {
        self.client
            .send_text(
                serde_json::to_string(&Message {
                    event: EventType::Ack,
                    data: serde_json::to_value(&AckResponse {
                        result: msg
                            .map(|txt| txt.to_string())
                            .unwrap_or("Message acknowledged".to_string()),
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
                        error: "Message is invalid. Rejected".to_string(),
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
        use EventType::*;

        match self.client.receive().await {
            Ok(Frame::Text { payload, .. }) => {
                let msg = if let Ok(msg) = serde_json::from_str::<Message>(&payload)
                    .map_err(|_| ConnectorError::InvalidMessage)
                {
                    msg
                } else {
                    self.send_error().await?;
                    return Ok(());
                };

                println!("DEBUG MSG: {:?}", msg);

                match msg.event {
                    NewJudgementRequest => {
                        println!("Received a new identity from Watcher!");
                        if let Ok(request) = serde_json::from_value::<JudgementRequest>(msg.data) {
                            if let Ok(_ident) = OnChainIdentity::try_from(request) {
                                self.send_ack(None).await?;
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
