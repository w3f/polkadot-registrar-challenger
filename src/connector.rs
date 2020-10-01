use crate::comms::{CommsMessage, CommsVerifier};
use crate::identity::OnChainIdentity;
use crate::primitives::{Account, AccountType, Judgement, NetAccount, Result};
use futures::sink::SinkExt;
use futures::stream::{SplitSink, SplitStream};
use futures::StreamExt;
use futures::{select_biased, FutureExt};
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
use futures_channel::mpsc::{unbounded, UnboundedSender, UnboundedReceiver};

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

        tokio::spawn(Self::start_comms_receiver(self.comms.clone(), sender.clone()));
        tokio::spawn(Self::start_websocket_writer(writer, self.comms.clone(), receiver));
        tokio::spawn(Self::start_websocket_reader(reader, self.comms.clone(), sender));
    }
    async fn start_comms_receiver(comms: CommsVerifier, mut sender: UnboundedSender<Message>) {
        loop {
            match comms.recv().await {
                CommsMessage::JudgeIdentity {
                    net_account,
                    judgement,
                } => {
                    sender.send(
                        Message {
                            event: EventType::JudgementResult,
                            data: serde_json::to_value(&JudgementResponse {
                                address: net_account.clone(),
                                judgement: judgement,
                            })
                            .map_err(|err| ConnectorError::Response(err.into())).unwrap(),
                        }
                    );
                }
                _ => {}
            }
        }
    }
    async fn start_websocket_writer(mut writer: SplitSink<WebSocketStream<TcpStream>, TungMessage>, comms: CommsVerifier, mut receiver: UnboundedReceiver<Message>) {
        loop {
            if let Some(message) = receiver.next().await {
                writer.send(TungMessage::Text(
                    serde_json::to_string(&message).unwrap()
                ));
            }
        }
    }
    async fn start_websocket_reader(mut reader: SplitStream<WebSocketStream<TcpStream>>, comms: CommsVerifier, sender: UnboundedSender<Message>) {
        loop {
            if let Some(message) = reader.next().await {
                // TODO: This must be initialized outside, use Cell.
                let mut receiver_error = false;

                message
                    .map_err(|err| {
                        match err {
                            TungError::ConnectionClosed => {
                                // Prevent spamming log messages if the server is
                                // disconnected.
                                if !receiver_error {
                                    error!("Disconnected from Listener: {}", err);
                                    receiver_error = true;
                                }

                                // Try to silently reconnect
                                // TODO...
                            }
                            _ => {
                                receiver_error = false;
                                error!("{}", err)
                            }
                        };
                    })
                    .map(|message| {
                        match message {
                            TungMessage::Text(payload) => {
                                trace!("Received message from Watcher: {}", payload);
                                let try_msg = serde_json::from_str::<Message>(&payload);
                                let msg = if let Ok(msg) = try_msg {
                                    msg
                                } else {
                                    // TODO: Respond with error

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
                                                comms.notify_new_identity(ident);
                                            } else {
                                                // TODO: Respond with error
                                            };
                                        } else {
                                            // TODO: Respond with error
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    });
            }
        }
    }
    /*
    fn send_ack(
        msg: Option<&str>,
        writer: &Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, TungMessage>>>,
    ) -> Result<()> {
        writer.lock().unwrap().send(TungMessage::Text(
            serde_json::to_string(&Message {
                event: EventType::Ack,
                data: serde_json::to_value(&AckResponse {
                    result: msg.unwrap_or("Message acknowledged").to_string(),
                })
                .map_err(|err| ConnectorError::Response(err.into()))?,
            })
            .map_err(|err| ConnectorError::Response(err.into()))?,
        ));

        Ok(())
    }
    fn send_error(
        writer: &Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, TungMessage>>>,
    ) -> Result<()> {
        writer.lock().unwrap().send(TungMessage::Text(
            serde_json::to_string(&Message {
                event: EventType::Error,
                data: serde_json::to_value(&ErrorResponse {
                    error: "Message is invalid. Rejected".to_string(),
                })
                .map_err(|err| ConnectorError::Response(err.into()))?,
            })
            .map_err(|err| ConnectorError::Response(err.into()))?,
        ));

        Ok(())
    }
    */
}
