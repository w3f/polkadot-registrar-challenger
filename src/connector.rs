use crate::comms::{CommsMessage, CommsVerifier};
use crate::manager::OnChainIdentity;
use crate::primitives::{Account, AccountType, Judgement, NetAccount, Result};
use futures::sink::SinkExt;
use futures::stream::{SplitSink, SplitStream};
use futures::{StreamExt, TryStreamExt};
use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
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
    #[serde(rename = "pendingJudgementsRequest")]
    PendingJudgementsRequests,
    #[serde(rename = "pendingJudgementsResponse")]
    PendingJudgementsResponse,
    #[serde(rename = "displayNamesRequest")]
    DisplayNamesRequest,
    #[serde(rename = "displayNamesResponse")]
    DisplayNamesResponse,
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

#[async_trait]
trait ConnectorReaderTransport {
    async fn read(&mut self) -> Result<Option<String>>;
}

#[async_trait]
trait ConnectorWriterTransport {
    async fn write(&mut self, message: &Message) -> Result<()>;
}

pub struct WebSocketReader {
    reader: SplitStream<WebSocketStream<TcpStream>>,
}

impl From<SplitStream<WebSocketStream<TcpStream>>> for WebSocketReader {
    fn from(reader: SplitStream<WebSocketStream<TcpStream>>) -> Self {
        WebSocketReader { reader: reader }
    }
}

#[async_trait]
impl ConnectorReaderTransport for WebSocketReader {
    async fn read(&mut self) -> Result<Option<String>> {
        if let Some(message) = self.reader.try_next().await? {
            match message {
                TungMessage::Text(payload) => Ok(Some(payload)),
                _ => Err(failure::err_msg("Not a text message")),
            }
        } else {
            Ok(None)
        }
    }
}

pub struct WebSocketWriter {
    writer: SplitSink<WebSocketStream<TcpStream>, TungMessage>,
}

impl From<SplitSink<WebSocketStream<TcpStream>, TungMessage>> for WebSocketWriter {
    fn from(writer: SplitSink<WebSocketStream<TcpStream>, TungMessage>) -> Self {
        WebSocketWriter { writer: writer }
    }
}

#[async_trait]
impl ConnectorWriterTransport for WebSocketWriter {
    async fn write(&mut self, message: &Message) -> Result<()> {
        self.writer
            .send(TungMessage::Text(serde_json::to_string(message)?))
            .await
            .map_err(|err| err.into())
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
            let exit_token = Arc::new(RwLock::new(false));

            tokio::spawn(Self::start_comms_receiver(
                self.comms.clone(),
                sender.clone(),
                Arc::clone(&exit_token),
            ));

            tokio::spawn(Self::start_websocket_writer(
                writer,
                self.comms.clone(),
                receiver,
                Arc::clone(&exit_token),
            ));

            let handle = tokio::spawn(Self::start_websocket_reader(
                reader,
                self.comms.clone(),
                sender.clone(),
                Arc::clone(&exit_token),
            ));

            info!("Requesting display names");
            sender
                .send(Message {
                    event: EventType::DisplayNamesRequest,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                })
                .await
                .map_err(|err| {
                    error!("Failed to fetch display names: {}", err);
                    std::process::exit(1);
                })
                .unwrap();

            info!("Requesting pending judgments");
            sender
                .send(Message {
                    event: EventType::PendingJudgementsRequests,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                })
                .await
                .map_err(|err| {
                    error!("Failed to fetch pending judgment requests: {}", err);
                    std::process::exit(1);
                })
                .unwrap();

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
        exit_token: Arc<RwLock<bool>>,
    ) {
        loop {
            match comms.try_recv() {
                Some(CommsMessage::JudgeIdentity {
                    net_account,
                    judgement,
                }) => {
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

            let t = exit_token.read().await;
            if *t {
                debug!("Closing websocket writer task");
                break;
            }
        }
    }
    async fn start_websocket_writer(
        mut writer: SplitSink<WebSocketStream<TcpStream>, TungMessage>,
        _comms: CommsVerifier,
        mut receiver: UnboundedReceiver<Message>,
        exit_token: Arc<RwLock<bool>>,
    ) {
        loop {
            if let Ok(msg) = time::timeout(Duration::from_millis(10), receiver.next()).await {
                writer
                    .send(TungMessage::Text(serde_json::to_string(&msg).unwrap()))
                    .await
                    .unwrap();
            }

            let t = exit_token.read().await;
            if *t {
                debug!("Closing websocket writer task");
                break;
            }
        }
    }
    async fn start_websocket_reader(
        mut reader: SplitStream<WebSocketStream<TcpStream>>,
        comms: CommsVerifier,
        mut sender: UnboundedSender<Message>,
        exit_token: Arc<RwLock<bool>>,
    ) {
        use EventType::*;

        loop {
            if let Some(message) = reader.next().await {
                if let Ok(message) = &message {
                    trace!("Received message: {:?}", message);

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
                                NewJudgementRequest => {
                                    info!("Received a new judgement request");
                                    if let Ok(request) =
                                        serde_json::from_value::<JudgementRequest>(msg.data)
                                    {
                                        if let Ok(ident) = OnChainIdentity::try_from(request) {
                                            comms.notify_new_identity(ident);
                                            sender.send(Message::ack(None)).await.unwrap();
                                        } else {
                                            error!("Invalid `newJudgementRequest` message format");
                                            sender.send(Message::error()).await.unwrap();
                                        };
                                    } else {
                                        error!("Invalid `newJudgementRequest` message format");
                                        sender.send(Message::error()).await.unwrap();
                                    }
                                }
                                Ack => {
                                    if let Ok(msg) = serde_json::from_value::<AckResponse>(msg.data)
                                    {
                                        info!("Received acknowledgement: {}", msg.result);
                                        comms.notify_ack();
                                    } else {
                                        error!("Invalid 'acknowledgement' message format");
                                    }
                                }
                                Error => {
                                    if let Ok(msg) =
                                        serde_json::from_value::<ErrorResponse>(msg.data)
                                    {
                                        error!("Received error message: {}", msg.error);
                                    } else {
                                        error!("Invalid 'error' message format");
                                    }
                                }
                                PendingJudgementsResponse => {
                                    info!("Received pending challenges");
                                    if let Ok(requests) =
                                        serde_json::from_value::<Vec<JudgementRequest>>(msg.data)
                                    {
                                        if requests.is_empty() {
                                            info!("The pending judgement list is empty");
                                        }

                                        for request in requests {
                                            if let Ok(ident) = OnChainIdentity::try_from(request) {
                                                sender.send(Message::ack(None)).await.unwrap();
                                                comms.notify_new_identity(ident);
                                                sender.send(Message::ack(None)).await.unwrap();
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
                                DisplayNamesResponse => {
                                    info!("Received display names response");
                                    debug!("Display names {:?}", msg.data);

                                    if let Ok(display_names) =
                                        serde_json::from_value::<Vec<Account>>(msg.data)
                                    {
                                        comms.notify_existing_display_names(display_names);
                                    } else {
                                        error!("Invalid `displayNamesResponse` message format");
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
                            let mut t = exit_token.write().await;
                            *t = true;

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
