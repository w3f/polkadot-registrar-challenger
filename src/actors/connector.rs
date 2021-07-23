use crate::primitives::{
    ChainAddress, ChainName, IdentityContext, IdentityFieldValue, JudgementState 
};
use crate::Database;
use crate::Result;
use actix::io::SinkWrite;
use actix::io::WriteHandler;
use actix::*;
use actix_codec::Framed;
use awc::{
    error::WsProtocolError,
    ws::{Codec, Frame, Message},
    BoxedSocket, Client,
};
use futures::stream::{SplitSink, StreamExt};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::sleep;

pub async fn run_connector(url: String, db: Database) -> Result<()> {
    info!("Setting up connector to Watcher at endpoint: {}", url);

    // Init processing queue.
    let (tx, recv) = unbounded_channel();

    let mut connector = init_connector(url.as_str(), tx.clone()).await?;

    // Run processing queue for incoming connector messages.
    info!("Starting processing queue for incoming messages");
    run_queue_processor(db.clone(), recv).await;

    info!("Starting judgments event loop");
    actix::spawn(async move {
        async fn local(
            connector: &mut Addr<Connector>,
            db: &Database,
            url: &str,
            tx: UnboundedSender<QueueMessage>,
        ) -> Result<()> {
            // If connection dropped, try to reconnect
            if !connector.connected() {
                warn!("Connection to Watcher dropped, trying to reconnect...");
                *connector = init_connector(url, tx).await?
            }

            // Provide judgments.
            let completed = db.fetch_completed_not_submitted().await?;
            for state in completed {
                debug!("Notifying Watcher about judgement: {:?}", state.context);
                connector.do_send(ClientCommand::ProvideJudgement(state.context));
            }

            Ok(())
        }

        loop {
            match local(
                &mut connector,
                &db,
                url.as_str(),
                tx.clone(),
            )
            .await
            {
                Ok(_) => {}
                Err(err) => {
                    error!("Connector error: {:?}", err);
                }
            }

            sleep(Duration::from_secs(1)).await;
        }
    });

    Ok(())
}

async fn run_queue_processor(db: Database, mut recv: UnboundedReceiver<QueueMessage>) {
    fn create_context(address: ChainAddress) -> IdentityContext {
        let chain = if address.as_str().starts_with("1") {
            ChainName::Polkadot
        } else {
            ChainName::Kusama
        };

        IdentityContext {
            address: address,
            chain: chain,
        }
    }

    async fn process_request(
        db: &Database,
        address: ChainAddress,
        accounts: HashMap<AccountType, String>,
    ) -> Result<()> {
        let id = create_context(address);
        let state = JudgementState::new(id, accounts.into_iter().map(|a| a.into()).collect());
        db.add_judgement_request(state).await?;

        Ok(())
    }

    async fn local(db: &Database, recv: &mut UnboundedReceiver<QueueMessage>) -> Result<()> {
        debug!("Watcher message picked up by queue");
        while let Some(message) = recv.recv().await {
            match message {
                QueueMessage::Ack(data) => {
                    // TODO: Check the "result"
                    if let Some(address) = data.address {
                        let context = create_context(address);
                        db.set_submitted(&context).await?;
                    }
                }
                QueueMessage::NewJudgementRequest(data) => {
                    process_request(&db, data.address, data.accounts)
                        .await?
                }
                QueueMessage::PendingJudgementsRequests(data) => {
                    for d in data {
                        process_request(&db, d.address, d.accounts).await?;
                    }
                }
            }
        }

        Ok(())
    }

    actix::spawn(async move {
        loop {
            let _ = local(&db, &mut recv)
                .await
                .map_err(|err| error!("Error in connector messaging queue: {:?}", err));

            sleep(Duration::from_secs(10)).await;
        }
    });
}

async fn init_connector(
    endpoint: &str,
    queue: UnboundedSender<QueueMessage>,
) -> Result<Addr<Connector>> {
    let (_, framed) = Client::new().ws(endpoint).connect().await.map_err(|err| {
        anyhow!(
            "failed to initiate client connector to {}: {:?}",
            endpoint,
            err
        )
    })?;

    let (sink, stream) = framed.split();
    let actor = Connector::create(|ctx| {
        Connector::add_stream(stream, ctx);
        Connector {
            sink: SinkWrite::new(sink, ctx),
            queue: queue,
        }
    });

    Ok(actor)
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResponseMessage<T> {
    pub event: EventType,
    pub data: T,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgementResponse {
    pub address: ChainAddress,
    pub judgement: Judgement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckResponse {
    result: String,
    address: Option<ChainAddress>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Judgement {
    #[serde(rename = "reasonable")]
    Reasonable,
    #[serde(rename = "erroneous")]
    Erroneous,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JudgementRequest {
    pub address: ChainAddress,
    pub accounts: HashMap<AccountType, String>,
}

#[derive(Debug, Clone)]
enum QueueMessage {
    Ack(AckResponse),
    NewJudgementRequest(JudgementRequest),
    PendingJudgementsRequests(Vec<JudgementRequest>),
}

struct Connector {
    sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    queue: UnboundedSender<QueueMessage>,
}

#[derive(Message)]
#[rtype(result = "()")]
enum ClientCommand {
    ProvideJudgement(IdentityContext),
}

impl Handler<ClientCommand> for Connector {
    type Result = ();

    fn handle(&mut self, msg: ClientCommand, _ctx: &mut Context<Self>) {
        match msg {
            ClientCommand::ProvideJudgement(id) => {
                self.sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::JudgementResult,
                        data: JudgementResponse {
                            address: id.address.clone(),
                            judgement: Judgement::Reasonable,
                        },
                    })
                    .unwrap(),
                ));
            }
        }
    }
}

impl Actor for Connector {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.heartbeat(ctx)
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        error!("Connector disconnected");
    }
}

impl Connector {
    fn heartbeat(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(5, 0), |act, ctx| {
            act.sink.write(Message::Ping(String::from("").into()));
            act.heartbeat(ctx);

            // TODO: Check timeouts
        });
    }
}

/// Handle server websocket messages
impl StreamHandler<std::result::Result<Frame, WsProtocolError>> for Connector {
    fn handle(
        &mut self,
        msg: std::result::Result<Frame, WsProtocolError>,
        _ctx: &mut Context<Self>,
    ) {
        let parsed: ResponseMessage<serde_json::Value> = if let Ok(Frame::Text(txt)) = msg {
            serde_json::from_slice(&txt).unwrap()
        } else {
            return;
        };

        match parsed.event {
            EventType::Ack => {
                let data: AckResponse = serde_json::from_value(parsed.data).unwrap();
                debug!("Received acknowledgement from Watcher: {:?}", data);

                self.queue.send(QueueMessage::Ack(data)).unwrap();
            }
            EventType::Error => {
                error!("Received error from Watcher: {:?}", parsed.data);
            }
            EventType::NewJudgementRequest => {
                let data: JudgementRequest = serde_json::from_value(parsed.data).unwrap();
                debug!("Received new judgement request from Watcher: {:?}", data);

                self.queue
                    .send(QueueMessage::NewJudgementRequest(data))
                    .unwrap();
            }
            EventType::PendingJudgementsResponse => {
                let data: Vec<JudgementRequest> = serde_json::from_value(parsed.data).unwrap();
                debug!("Received pending judgments from Watcher: {:?}", data);

                self.queue
                    .send(QueueMessage::PendingJudgementsRequests(data))
                    .unwrap();
            }
            _ => {
                warn!("Received unrecognized message from watcher: {:?}", parsed);
            }
        }
    }

    fn started(&mut self, _ctx: &mut Context<Self>) {
        info!("Connector started");
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        error!("Watcher disconnected");
        ctx.stop()
    }
}

impl WriteHandler<WsProtocolError> for Connector {}

#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize)]
pub enum AccountType {
    #[serde(rename = "legal_name")]
    LegalName,
    #[serde(rename = "display_name")]
    DisplayName,
    #[serde(rename = "email")]
    Email,
    #[serde(rename = "web")]
    Web,
    #[serde(rename = "twitter")]
    Twitter,
    #[serde(rename = "matrix")]
    Matrix,
    #[serde(rename = "pgpFingerprint")]
    PGPFingerprint,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "additional")]
    Additional,
}

impl From<(AccountType, String)> for IdentityFieldValue {
    fn from(val: (AccountType, String)) -> Self {
        let (ty, value) = val;

        match ty {
            AccountType::LegalName => IdentityFieldValue::LegalName(value),
            AccountType::DisplayName => IdentityFieldValue::DisplayName(value),
            AccountType::Email => IdentityFieldValue::Email(value),
            AccountType::Web => IdentityFieldValue::Web(value),
            AccountType::Twitter => IdentityFieldValue::Twitter(value),
            AccountType::Matrix => IdentityFieldValue::Matrix(value),
            AccountType::PGPFingerprint => IdentityFieldValue::PGPFingerprint(()),
            AccountType::Image => IdentityFieldValue::Image(()),
            AccountType::Additional => IdentityFieldValue::Additional(()),
        }
    }
}
