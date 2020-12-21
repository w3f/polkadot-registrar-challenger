use crate::comms::{CommsMessage, CommsVerifier};
use crate::manager::OnChainIdentity;
use crate::primitives::{unix_time, Account, AccountType, Judgement, NetAccount, Result};
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventType {
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub event: EventType,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub data: Value,
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
pub(crate) struct AckResponse {
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JudgementResponse {
    pub address: NetAccount,
    pub judgement: Judgement,
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct JudgementGiven {
    address: NetAccount,
    result: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DisplayNamesEntry {
    display_name: Account,
    address: NetAccount,
}

#[async_trait]
pub trait ConnectorInitTransports<W: ConnectorWriterTransport, R: ConnectorReaderTransport> {
    type Endpoint;

    async fn init(endpoint: Self::Endpoint) -> Result<(W, R)>;
}

#[async_trait]
pub trait ConnectorReaderTransport {
    async fn read(&mut self) -> Result<Option<String>>;
}

#[async_trait]
pub trait ConnectorWriterTransport {
    async fn write(&mut self, message: &Message) -> Result<()>;
}

pub struct WebSockets {}

#[async_trait]
impl ConnectorInitTransports<WebSocketWriter, WebSocketReader> for WebSockets {
    type Endpoint = String;

    async fn init(endpoint: Self::Endpoint) -> Result<(WebSocketWriter, WebSocketReader)> {
        let (sink, stream) = connect_async(endpoint.as_str()).await?.0.split();

        Ok((sink.into(), stream.into()))
    }
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
                TungMessage::Text(message) => Ok(Some(message)),
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

pub struct Connector<W, R, P> {
    writer: W,
    reader: R,
    comms: CommsVerifier,
    endpoint: P,
}

impl<
        W: 'static + Send + Sync + ConnectorWriterTransport,
        R: 'static + Send + Sync + ConnectorReaderTransport,
        P: 'static + Send + Sync + Clone,
    > Connector<W, R, P>
{
    pub async fn new<T: ConnectorInitTransports<W, R, Endpoint = P>>(
        endpoint: P,
        comms: CommsVerifier,
    ) -> Result<Self> {
        let (writer, reader) = T::init(endpoint.clone()).await?;

        Ok(Connector {
            writer: writer,
            reader: reader,
            comms: comms,
            endpoint: endpoint,
        })
    }
    #[cfg(test)]
    pub fn set_writer_reader(&mut self, writer: W, reader: R) {
        self.writer = writer;
        self.reader = reader;
    }
    pub async fn start<T: ConnectorInitTransports<W, R, Endpoint = P>>(mut self) {
        loop {
            let (mut sender, receiver) = unbounded();
            let exit_token = Arc::new(RwLock::new(false));

            tokio::spawn(Self::start_comms_receiver(
                self.comms.clone(),
                sender.clone(),
                Arc::clone(&exit_token),
            ));

            tokio::spawn(Self::start_websocket_writer(
                self.writer,
                self.comms.clone(),
                receiver,
                Arc::clone(&exit_token),
            ));

            let handle = tokio::spawn(Self::start_websocket_reader(
                self.reader,
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
                if let Ok((writer, reader)) = T::init(self.endpoint.clone()).await {
                    info!("Connected successfully to Watcher, spawning tasks");
                    self.writer = writer;
                    self.reader = reader;

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
    async fn start_websocket_writer<T: ConnectorWriterTransport>(
        mut transport: T,
        _comms: CommsVerifier,
        mut receiver: UnboundedReceiver<Message>,
        exit_token: Arc<RwLock<bool>>,
    ) {
        let mut last_check = unix_time();

        loop {
            if let Ok(msg) = time::timeout(Duration::from_millis(10), receiver.next()).await {
                if let Some(msg) = msg {
                    let _ = transport.write(&msg).await.map_err(|err| {
                        error!("{}", err);
                    });
                }
            }

            // Ping the Watcher every minute. Serves as a keep-alive mechanism.
            let now = unix_time();
            if now >= last_check + 60 {
                debug!("Keep-alive ping");
                let _ = transport
                    .write(&Message {
                        event: EventType::PendingJudgementsRequests,
                        data: serde_json::to_value(Option::<()>::None).unwrap(),
                    })
                    .await
                    .map_err(|err| {
                        error!("{}", err);
                    });

                last_check = now;
            }

            let t = exit_token.read().await;
            if *t {
                debug!("Closing websocket writer task");
                break;
            }
        }
    }
    async fn start_websocket_reader<T: ConnectorReaderTransport>(
        mut transport: T,
        comms: CommsVerifier,
        mut sender: UnboundedSender<Message>,
        exit_token: Arc<RwLock<bool>>,
    ) {
        use EventType::*;

        loop {
            if let Ok(message) = transport.read().await {
                if let Some(message) = message {
                    trace!("Received message: {:?}", message);

                    let try_msg = serde_json::from_str::<Message>(&message);
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
                            if let Ok(msg) = serde_json::from_value::<AckResponse>(msg.data.clone())
                            {
                                trace!("Received acknowledgement: {}", msg.result);
                                comms.notify_ack();
                            } else if let Ok(msg) =
                                serde_json::from_value::<JudgementGiven>(msg.data)
                            {
                                if msg.result.to_lowercase() == "judgement given" {
                                    info!(
                                        "Received judgement acknowledgement for address: {}",
                                        msg.address.as_str()
                                    );
                                    comms.notify_judgement_given_ack(msg.address)
                                } else {
                                    error!("Invalid 'acknowledgement' message format");
                                }
                            } else {
                                error!("Invalid 'acknowledgement' message format");
                            }
                        }
                        Error => {
                            if let Ok(msg) = serde_json::from_value::<ErrorResponse>(msg.data) {
                                error!("Received error message: {}", msg.error);
                            } else {
                                error!("Invalid 'error' message format");
                            }
                        }
                        PendingJudgementsResponse => {
                            trace!("Received pending challenges");
                            if let Ok(requests) =
                                serde_json::from_value::<Vec<JudgementRequest>>(msg.data)
                            {
                                if requests.is_empty() {
                                    trace!("The pending judgement list is empty. Waiting...");
                                } else {
                                    info!("Pending judgement requests: {:?}", requests);
                                }

                                for request in requests {
                                    if let Ok(ident) = OnChainIdentity::try_from(request) {
                                        sender.send(Message::ack(None)).await.unwrap();
                                        comms.notify_new_identity(ident);
                                        sender.send(Message::ack(None)).await.unwrap();
                                    } else {
                                        error!("Invalid `newJudgementRequest` message format");
                                        sender.send(Message::error()).await.unwrap();
                                    };
                                }
                            } else {
                                error!("Invalid `pendingChallengesRequest` message format");
                            }
                        }
                        DisplayNamesResponse => {
                            trace!("Received display names response");
                            trace!("Display names {:?}", msg.data);

                            if let Ok(display_names) =
                                serde_json::from_value::<Vec<DisplayNamesEntry>>(msg.data)
                            {
                                comms.notify_existing_display_names(
                                    display_names
                                        .into_iter()
                                        .map(|e| (e.display_name, e.address))
                                        .collect(),
                                );
                            } else {
                                error!("Invalid `displayNamesResponse` message format");
                            }
                        }
                        _ => {
                            warn!("Received unrecognized message: '{:?}'", msg);
                        }
                    }
                }
            } else {
                warn!("Connection to Watcher closed");
                debug!("Closing websocket reader task");

                // Close Comms receiver and client writer tasks.
                let mut t = exit_token.write().await;
                *t = true;

                break;
            }
        }
    }
}
