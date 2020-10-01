use crate::comms::{CommsMessage, CommsVerifier};
use crate::identity::OnChainIdentity;
use crate::primitives::{Account, AccountType, Judgement, NetAccount, Result};
use futures::sink::SinkExt;
use futures::stream::{SplitSink, SplitStream};
use futures::StreamExt;
use futures::{select_biased, FutureExt};
use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::result::Result as StdResult;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio::time::{self, Duration};
use tokio_tungstenite::{connect_async, WebSocketStream};
use tungstenite::error::Error as TungError;
use tungstenite::protocol::Message as TungMessage;
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

impl Message {
    fn ack(msg: Option<&str>) -> Message {
        Message {
            event: EventType::Ack,
            data: serde_json::to_value(&AckResponse {
                result: msg.unwrap_or("Message acknowledged").to_string(),
            })
            .unwrap(),
        }
    }
    fn error() -> Message {
        Message {
            event: EventType::Error,
            data: serde_json::to_value(&ErrorResponse {
                error: "Message is invalid. Rejected".to_string(),
            })
            .unwrap(),
        }
    }
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
    client: WebSocketStream<TcpStream>,
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
    UnboundedReceiver(failure::Error),
}

impl Connector {
    pub async fn new(url: &str, comms: CommsVerifier) -> Result<Self> {
        let mut connector = Connector {
            client: connect_async(url).await?.0,
            comms: comms,
            url: url.to_owned(),
        };

        Ok(connector)
    }
    pub async fn start(self) {
        let (writer, reader) = self.client.split();
        let (sender, receiver) = unbounded();

        tokio::spawn(Self::start_comms_receiver(
            self.comms.clone(),
            sender.clone(),
        ));
        tokio::spawn(Self::start_websocket_writer(
            writer,
            self.comms.clone(),
            receiver,
        ));
        tokio::spawn(Self::start_websocket_reader(
            reader,
            self.comms.clone(),
            sender,
        ));
    }
    async fn start_comms_receiver(comms: CommsVerifier, mut sender: UnboundedSender<Message>) {
        loop {
            match comms.recv().await {
                CommsMessage::JudgeIdentity {
                    net_account,
                    judgement,
                } => {
                    sender
                        .send(Message {
                            event: EventType::JudgementResult,
                            data: serde_json::to_value(&JudgementResponse {
                                address: net_account.clone(),
                                judgement: judgement,
                            })
                            .map_err(|err| ConnectorError::Response(err.into()))
                            .unwrap(),
                        })
                        .await;
                }
                _ => {}
            }
        }
    }
    async fn start_websocket_writer(
        mut writer: SplitSink<WebSocketStream<TcpStream>, TungMessage>,
        comms: CommsVerifier,
        mut receiver: UnboundedReceiver<Message>,
    ) {
        loop {
            if let Some(message) = receiver.next().await {
                writer
                    .send(TungMessage::Text(serde_json::to_string(&message).unwrap()))
                    .await
                    .unwrap();
            }
        }
    }
    async fn start_websocket_reader(
        mut reader: SplitStream<WebSocketStream<TcpStream>>,
        comms: CommsVerifier,
        mut sender: UnboundedSender<Message>,
    ) {
        loop {
            if let Some(message) = reader.next().await {
                if let Ok(message) = &message {
                    match message {
                        TungMessage::Text(payload) => {
                            trace!("Received message from Watcher: {}", payload);
                            let try_msg = serde_json::from_str::<Message>(&payload);
                            let msg = if let Ok(msg) = try_msg {
                                msg
                            } else {
                                sender.send(Message::error()).await.unwrap();

                                return;
                            };

                            match msg.event {
                                EventType::NewJudgementRequest => {
                                    info!("Received a new judgement request from Watcher");
                                    if let Ok(request) =
                                        serde_json::from_value::<JudgementRequest>(msg.data)
                                    {
                                        if let Ok(ident) = OnChainIdentity::try_from(request) {
                                            // TODO: Respond with acknowledgement
                                            sender.send(Message::ack(None)).await.unwrap();
                                            comms.notify_new_identity(ident);
                                        } else {
                                            sender.send(Message::error()).await.unwrap();
                                        };
                                    } else {
                                        sender.send(Message::error()).await.unwrap();
                                    }
                                }
                                _ => {
                                    sender.send(Message::error()).await.unwrap();
                                }
                            }
                        }
                        _ => {
                            sender.send(Message::error()).await.unwrap();
                        }
                    }
                }
                if let Err(err) = message {
                    error!("{}", err);
                }
            }
        }
    }
}
