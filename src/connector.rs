use crate::comms::{CommsMessage, CommsVerifier};
use crate::identity::OnChainIdentity;
use crate::primitives::{Account, AccountType, Judgement, NetAccount, Result};
use futures::sink::SinkExt;
use futures::stream::{SplitSink, SplitStream};
use futures::FutureExt;
use futures::StreamExt;
use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryFrom;
use tokio::net::TcpStream;
use tokio::time::{self, Duration};
use tokio_tungstenite::{connect_async, WebSocketStream};
use tungstenite::protocol::Message as TungMessage;
use tungstenite::Error as TungError;

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
    #[serde(rename = "pendingJudgementsRequests")]
    PendingJudgementsRequests,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Message {
    event: EventType,
    #[serde(skip_serializing_if = "Value::is_null")]
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
    pub accounts: HashMap<AccountType, Option<Account>>,
}

impl TryFrom<JudgementRequest> for OnChainIdentity {
    type Error = failure::Error;

    fn try_from(request: JudgementRequest) -> Result<Self> {
        let mut ident = OnChainIdentity::new(request.address)?;

        for (account_ty, account) in request.accounts {
            if let Some(account) = account {
                ident.push_account(account_ty, account)?;
            }
        }

        Ok(ident)
    }
}

pub struct Connector {
    client: WebSocketStream<TcpStream>,
    comms: CommsVerifier,
    url: String,
}

impl Connector {
    pub async fn new(url: &str, comms: CommsVerifier) -> Result<Self> {
        let connector = Connector {
            client: connect_async(url).await?.0,
            comms: comms,
            url: url.to_string(),
        };

        Ok(connector)
    }
    pub async fn start(self) {
        let (mut writer, mut reader) = self.client.split();

        loop {
            let (mut sender, receiver) = unbounded();

            // Special channels for coordinating the exit of the tasks in case
            // of Watcher disconnect.
            let (tx1, rx1) = unbounded();
            let (tx2, rx2) = unbounded();

            tokio::spawn(Self::start_comms_receiver(
                self.comms.clone(),
                sender.clone(),
                rx1,
            ));

            tokio::spawn(Self::start_websocket_writer(
                writer,
                self.comms.clone(),
                receiver,
                rx2,
            ));

            let handle = tokio::spawn(Self::start_websocket_reader(
                reader,
                self.comms.clone(),
                sender.clone(),
                tx1,
                tx2,
            ));

            // Request pending requests from Watcher.
            /*
            info!("Requesting pending judgments from Watcher");
            sender
                .send(Message {
                    event: EventType::PendingJudgementsRequests,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                })
                .await
                .unwrap();
            */

            // Wait for the reader to exit, which in return will close the comms
            // receiver and writer task. This occurs when the connection to the
            // Watcher is closed.
            handle.await.unwrap();

            let mut interval = time::interval(Duration::from_secs(5));

            info!("Trying to reconnect to Watcher...");
            loop {
                interval.tick().await;
                if let Ok(client) = connect_async(&self.url).await {
                    info!("Connected successfully to Watcher, spawning tasks");

                    // (Destructuring assignments are currently not supported.
                    // https://github.com/rust-lang/rfcs/issues/372)
                    let split = client.0.split();
                    writer = split.0;
                    reader = split.1;

                    break;
                } else {
                    trace!("Connection to Watcher failed...");
                }
            }
        }
    }
    async fn start_comms_receiver(
        comms: CommsVerifier,
        mut sender: UnboundedSender<Message>,
        mut closure: UnboundedReceiver<bool>,
    ) {
        let mut closure_recv = closure.next().fuse();
        let mut comms_recv = Box::pin(comms.recv().fuse());

        loop {
            futures::select_biased! {
                msg = comms_recv => {
                    match msg {
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
                                    .unwrap(),
                                })
                                .await
                                .unwrap();
                        }
                        _ => {}
                    }
                }
                token = closure_recv => {
                    if let Some(_) = token {
                        debug!("Closing websocket comms task");
                        break;
                    }
                }
            };
        }
    }
    async fn start_websocket_writer(
        mut writer: SplitSink<WebSocketStream<TcpStream>, TungMessage>,
        _comms: CommsVerifier,
        mut receiver: UnboundedReceiver<Message>,
        mut closure: UnboundedReceiver<bool>,
    ) {
        let mut closure_recv = closure.next().fuse();
        let mut receiver_recv = receiver.next().fuse();

        loop {
            futures::select_biased! {
                msg = receiver_recv => {
                    if let Some(msg) = msg {
                        writer
                            .send(TungMessage::Text(serde_json::to_string(&msg).map(|s| {println!("{}", s); s}).unwrap()))
                            .await
                            .unwrap();
                    }
                }
                token = closure_recv => {
                    if let Some(_) = token {
                        debug!("Closing websocket writer task");
                        break;
                    }
                }
            }
        }
    }
    async fn start_websocket_reader(
        mut reader: SplitStream<WebSocketStream<TcpStream>>,
        comms: CommsVerifier,
        mut sender: UnboundedSender<Message>,
        mut tx1: UnboundedSender<bool>,
        mut tx2: UnboundedSender<bool>,
    ) {
        loop {
            if let Some(message) = reader.next().await {
                if let Ok(message) = &message {
                    match message {
                        TungMessage::Text(payload) => {
                            let try_msg = serde_json::from_str::<Message>(&payload);
                            let msg = if let Ok(msg) = try_msg {
                                msg
                            } else {
                                sender.send(Message::error()).await.unwrap();

                                return;
                            };

                            match msg.event {
                                EventType::NewJudgementRequest => {
                                    info!("Received a new judgement request");
                                    if let Ok(request) =
                                        serde_json::from_value::<JudgementRequest>(msg.data)
                                    {
                                        if let Ok(ident) = OnChainIdentity::try_from(request) {
                                            sender.send(Message::ack(None)).await.unwrap();
                                            comms.notify_new_identity(ident);
                                        } else {
                                            error!("Invalid `newJudgementRequest` message format");
                                            sender.send(Message::error()).await.unwrap();
                                        };
                                    } else {
                                        error!("Invalid `newJudgementRequest` message format");
                                        sender.send(Message::error()).await.unwrap();
                                    }
                                }
                                EventType::Ack => {
                                    if let Ok(msg) = serde_json::from_value::<AckResponse>(msg.data)
                                    {
                                        info!("Received acknowledgement: {}", msg.result);
                                        comms.notify_ack();
                                    } else {
                                        error!("Invalid 'acknowledgement' message format");
                                    }
                                }
                                EventType::Error => {
                                    if let Ok(msg) =
                                        serde_json::from_value::<ErrorResponse>(msg.data)
                                    {
                                        error!(
                                            "Received error message from Watcher: {}",
                                            msg.error
                                        );
                                    } else {
                                        error!("Invalid 'error' message format");
                                    }
                                }
                                EventType::PendingJudgementsRequests => {
                                    info!("Received pending challenges from Watcher");
                                    if let Ok(requests) =
                                        serde_json::from_value::<Vec<JudgementRequest>>(msg.data)
                                    {
                                        for request in requests {
                                            if let Ok(ident) = OnChainIdentity::try_from(request) {
                                                sender.send(Message::ack(None)).await.unwrap();
                                                comms.notify_new_identity(ident);
                                            } else {
                                                error!(
                                                    "Invalid `newJudgementRequest` message format"
                                                );
                                                sender.send(Message::error()).await.unwrap();
                                            };
                                        }
                                    } else {
                                        error!("Invalid `pendingChallengesRequest` message format");
                                    }
                                }
                                _ => {
                                    warn!("Received unrecognized message: '{:?}'", msg);
                                }
                            }
                        }
                        _ => {
                            error!("Failed to convert message");
                            sender.send(Message::error()).await.unwrap();
                        }
                    }
                }

                if let Err(err) = message {
                    match err {
                        TungError::ConnectionClosed | TungError::Protocol(_) => {
                            warn!("Connection to Watcher closed");
                            debug!("Closing websocket reader task");

                            // Close Comms receiver and client writer tasks.
                            tx1.send(true).await.unwrap();
                            tx2.send(true).await.unwrap();

                            break;
                        }
                        _ => {
                            error!("Connection error with Watcher: {}", err);
                        }
                    }
                }
            }
        }
    }
}
