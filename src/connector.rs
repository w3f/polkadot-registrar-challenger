use crate::comms::{CommsMessage, CommsVerifier};
use crate::identity::OnChainIdentity;
use crate::primitives::{Account, AccountType, Judgement, NetAccount, Result};
use futures::{select_biased, FutureExt};
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::result::Result as StdResult;
use websockets::{Frame, WebSocket};
use tokio::time::{self, Duration};
use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio::net::TcpStream;
use futures::StreamExt;
use futures::stream::{SplitStream, SplitSink};
use tungstenite::protocol::Message as TungMessage;
use tungstenite::error::Error as TungError;

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
pub(crate) struct JudgementRequest {
    pub address: NetAccount,
    pub accounts: HashMap<AccountType, Account>,
}

impl TryFrom<JudgementRequest> for OnChainIdentity {
    type Error = failure::Error;

    fn try_from(request: JudgementRequest) -> Result<Self> {
        let mut ident = OnChainIdentity::new(request.address)?;

        for (account_ty, account) in request.accounts {
            ident.push_account(account_ty, account)?;
        }

        Ok(ident)
    }
}

pub struct Connector {
    client: WebSocket,
    comms: CommsVerifier,
    url: String,
}

#[derive(Debug, Fail)]
enum ConnectorError {
    #[fail(display = "the received message is invalid: {}", 0)]
    InvalidMessage(failure::Error),
    #[fail(display = "failed to respond: {}", 0)]
    Response(failure::Error),
    #[fail(display = "failed to fetch messages from the listener: {}", 0)]
    Receiver(failure::Error),
}

impl Connector {
    pub async fn new(url: &str, comms: CommsVerifier) -> Result<Self> {
        let mut connector = Connector {
            client: WebSocket::connect(url).await?,
            comms: comms,
            url: url.to_owned(),
        };

        connector.send_ack(Some("Connection established")).await?;

        Ok(connector)
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
                    .map_err(|err| ConnectorError::Response(err.into()))?,
                })
                .map_err(|err| ConnectorError::Response(err.into()))?,
            )
            .await
            .map_err(|err| ConnectorError::Response(err.into()))
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
                    .map_err(|err| ConnectorError::Response(err.into()))?,
                })
                .map_err(|err| ConnectorError::Response(err.into()))?,
            )
            .await
            .map_err(|err| ConnectorError::Response(err.into()))
            .map(|_| ())
    }
    pub async fn start(mut self) {
        let (client, _) = connect_async(&self.url).await.unwrap();
        let (write, read) = client.split();
 
        self.start_comms_receiver(&write);

        let mut receiver_error = false;
        loop {
            if let Err(err) = self.local(&read).await {
                match err {
                    ConnectorError::Receiver(err) => {
                        // Prevent spamming log messages if the server is
                        // disconnected.
                        if !receiver_error {
                            error!("Disconnected from Listener: {}", err);
                            receiver_error = true;
                        }

                        // Try to silently reconnect
                        WebSocket::connect(&self.url)
                            .await
                            .map(|client| {
                                info!("Reconnected to Watcher");
                                self.client = client;
                                receiver_error = false;
                            })
                            .unwrap_or(());
                    }
                    _ => {
                        receiver_error = false;

                        error!("{}", err);
                    }
                }
            };
        }
    }
    fn start_comms_receiver(&self, writer: &SplitSink<WebSocketStream<TcpStream>, TungMessage>) {
        tokio::spawn(async move {
            loop {

            }
        });
    }
    async fn handle_comms_message(&self) -> Result<()> {
        /*
        match self.comms.recv().await {
            CommsMessage::JudgeIdentity {
                net_account,
                judgement,
            } => {
                self.client
                    .send_text(
                        serde_json::to_string(&Message {
                            event: EventType::JudgementResult,
                            data: serde_json::to_value(&JudgementResponse {
                                address: net_account.clone(),
                                judgement: judgement,
                            })
                            .map_err(|err| ConnectorError::Response(err.into()))?,
                        })
                        .map_err(|err| ConnectorError::Response(err.into()))?,
                    )
                    .await
                    .map_err(|err| ConnectorError::Response(err.into()))?;
            }
            _ => panic!("Received invalid message in Connector"),
        }
        */

        Ok(())
    }
    async fn local(&mut self, reader: &SplitStream<WebSocketStream<TcpStream>>) -> StdResult<(), ConnectorError> {
        use EventType::*;

        select_biased! {
            frame = self.client.receive().fuse() => {
                match frame.map_err(|err| ConnectorError::Receiver(err.into()))? {
                    Frame::Text { payload, .. } => {
                        trace!("Received message from Watcher: {}", payload);

                        let try_msg = serde_json::from_str::<Message>(&payload);
                        let msg = if let Ok(msg) = try_msg {
                            msg
                        } else {
                            self.send_error().await?;
                            return Err(ConnectorError::InvalidMessage(try_msg.unwrap_err().into()));
                        };

                        match msg.event {
                            NewJudgementRequest => {
                                info!("Received a new judgement request from Watcher");
                                if let Ok(request) =
                                    serde_json::from_value::<JudgementRequest>(msg.data)
                                {
                                    let ident = if let Ok(ident) = OnChainIdentity::try_from(request) {
                                        self.send_ack(None).await?;

                                        self.comms.notify_new_identity(ident);
                                    } else {
                                        self.send_error().await?;
                                    };
                                } else {
                                    self.send_error().await?;
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        self.send_error().await?;
                    }
                }
            }
        };

        Ok(())
    }
}
