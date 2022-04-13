use crate::display_name::DisplayNameVerifier;
use crate::primitives::{
    ChainAddress, ChainName, IdentityContext, IdentityFieldValue, JudgementState,
};
use crate::{Database, DisplayNameConfig, Result, WatcherConfig};
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

pub async fn run_connector(
    db: Database,
    watchers: Vec<WatcherConfig>,
    dn_config: DisplayNameConfig,
) -> Result<()> {
    if watchers.is_empty() {
        warn!("No watcher is configured. Cannot process any requests or issue judgments");
        return Ok(());
    }

    for config in watchers {
        info!("Initializing connection to Watcher: {:?}", config);

        let db = db.clone();

        // TODO: Handle error
        // Connector::start("", db).await;

        info!("Starting judgments event loop");
        /*
        actix::spawn(async move {
            async fn local(
                connector: &mut Addr<Connector>,
                db: &Database,
                config: &WatcherConfig,
                tx: UnboundedSender<WatcherMessage>,
            ) -> Result<()> {
                // Provide judgments.
                let completed = db.fetch_judgement_candidates().await?;
                for state in completed {
                    if state.context.chain == config.network {
                        debug!("Notifying Watcher about judgement: {:?}", state.context);
                        connector.do_send(ClientCommand::ProvideJudgement(state.context));
                    } else {
                        debug!(
                            "Skipping judgement on connector assigned to {:?}: {:?}",
                            config.network, state.context
                        );
                    }
                }

                // Request current/active display names.
                debug!("Requesting pending display names from Watcher");
                connector.do_send(ClientCommand::RequestDisplayNames);

                debug!("Requesting pending judgments from Watcher");
                connector.do_send(ClientCommand::RequestPendingJudgements);

                Ok(())
            }

            loop {
                match local(&mut connector, &db, &config, tx.clone()).await {
                    Ok(_) => {}
                    Err(err) => {
                        error!("Connector error: {:?}", err);
                    }
                }

                sleep(Duration::from_secs(10)).await;
            }
        });
        */
    }

    Ok(())
}

/// Convenience function for creating a full identity context when only the
/// address itself is present. Only supports Kusama and Polkadot for now.
pub fn create_context(address: ChainAddress) -> IdentityContext {
    let chain = if address.as_str().starts_with('1') {
        ChainName::Polkadot
    } else {
        ChainName::Kusama
    };

    IdentityContext { address, chain }
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
    PendingJudgementsRequest,
    #[serde(rename = "pendingJudgementsResponse")]
    PendingJudgementsResponse,
    #[serde(rename = "displayNamesRequest")]
    DisplayNamesRequest,
    #[serde(rename = "displayNamesResponse")]
    DisplayNamesResponse,
    #[serde(rename = "judgementUnrequested")]
    JudgementUnrequested,
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

// TODO: Move to primitives.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DisplayNameEntry {
    pub context: IdentityContext,
    pub display_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
/// The entry as sent by the Watcher. Then converted into `DisplayNameEntry`.
pub struct DisplayNameEntryRaw {
    pub address: ChainAddress,
    #[serde(alias = "displayName")]
    pub display_name: String,
}

impl DisplayNameEntryRaw {
    /// Display names with emojis are represented in HEX form. Decode the
    /// display name, assuming it can be decoded...
    pub fn try_decode_hex(&mut self) {
        try_decode_hex(&mut self.display_name);
    }
}

fn try_decode_hex(display_name: &mut String) {
    if display_name.starts_with("0x") {
        // Might be a false positive. Leave it as is if it cannot be decoded.
        if let Ok(name) = hex::decode(&display_name[2..]) {
            if let Ok(name) = String::from_utf8(name) {
                *display_name = name;
            }
        }
    }
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "crate::Result<()>")]
enum WatcherMessage {
    Ack(AckResponse),
    NewJudgementRequest(JudgementRequest),
    PendingJudgementsRequests(Vec<JudgementRequest>),
    ActiveDisplayNames(Vec<DisplayNameEntryRaw>),
}

#[derive(Message)]
#[rtype(result = "()")]
enum ClientCommand {
    ProvideJudgement(IdentityContext),
    RequestPendingJudgements,
    RequestDisplayNames,
}

/// Handles incoming and outgoing websocket messages to and from the Watcher.
struct Connector {
    sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    queue: UnboundedSender<WatcherMessage>,
    db: Database,
    dn_verifier: DisplayNameVerifier,
}

impl Connector {
    async fn start(endpoint: &str, db: Database, dn_verifier: DisplayNameVerifier) -> Result<()> {
        let (queue, recv) = unbounded_channel();

        let (_, framed) = Client::builder()
            .timeout(Duration::from_secs(120))
            .finish()
            .ws(endpoint)
            .connect()
            .await
            .map_err(|err| {
                anyhow!(
                    "failed to initiate client connector to {}: {:?}",
                    endpoint,
                    err
                )
            })?;

        // Start the Connector actor.
        let (sink, stream) = framed.split();
        let _ = Connector::create(|ctx| {
            Connector::add_stream(stream, ctx);
            Connector {
                sink: SinkWrite::new(sink, ctx),
                queue,
                db,
                dn_verifier,
            }
        });

        Ok(())
    }
    // Send a heartbeat to the Watcher every couple of seconds.
    fn start_heartbeat_sync(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::new(30, 0), |act, _ctx| {
            let _ = act
                .sink
                .write(Message::Ping(String::from("").into()))
                .map_err(|err| error!("Failed to send heartbeat to Watcher: {:?}", err));
        });
    }
    // Request pending judgements every couple of seconds.
    fn start_pending_judgements_sync(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::new(10, 0), |_act, ctx| {
            ctx.address()
                .do_send(ClientCommand::RequestPendingJudgements)
        });
    }
    // Request pending judgements every couple of seconds.
    fn start_judgement_candidates_task(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::new(10, 0), |_act, ctx| {
            ctx.address()
                .do_send(ClientCommand::RequestPendingJudgements)
        });
    }
}

impl Actor for Connector {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.start_heartbeat_sync(ctx);
        self.start_pending_judgements_sync(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        error!("Connector disconnected");
    }
}

// Handle messages that should be sent to the Watcher.
impl Handler<WatcherMessage> for Connector {
    type Result = ResponseActFuture<Self, crate::Result<()>>;

    fn handle(&mut self, msg: WatcherMessage, _ctx: &mut Context<Self>) -> Self::Result {
        async fn process_request(
            db: &Database,
            address: ChainAddress,
            mut accounts: HashMap<AccountType, String>,
            dn_verifier: &DisplayNameVerifier,
        ) -> Result<()> {
            let id = create_context(address);

            // Decode display name if appropriate.
            if let Some((_, val)) = accounts
                .iter_mut()
                .find(|(ty, _)| *ty == &AccountType::DisplayName)
            {
                try_decode_hex(val);
            }

            let state = JudgementState::new(id, accounts.into_iter().map(|a| a.into()).collect());
            db.add_judgement_request(&state).await?;
            dn_verifier.verify_display_name(&state).await?;

            Ok(())
        }

        let db = self.db.clone();
        let dn_verifier = self.dn_verifier.clone();

        Box::pin(
            async move {
                match msg {
                    WatcherMessage::Ack(data) => {
                        if data.result.to_lowercase().contains("judgement given") {
                            let context = create_context(data.address.ok_or_else(|| {
                                anyhow!(
                                    "no address specified in 'judgement given' response from Watcher"
                                )
                            })?);

                            info!("Marking {:?} as judged", context);
                            db.set_judged(&context).await?;
                        }
                    }
                    WatcherMessage::NewJudgementRequest(data) => {
                        process_request(&db, data.address, data.accounts, &dn_verifier).await?
                    }
                    WatcherMessage::PendingJudgementsRequests(data) => {
                        // Process any tangling submissions, meaning any verified
                        // requests that were submitted to the Watcher but the
                        // issued extrinsic was not direclty confirmed back. This
                        // usually should not happen, but can.
                        let addresses: Vec<&ChainAddress> =
                            data.iter().map(|state| &state.address).collect();
                        db.process_tangling_submissions(&addresses).await?;

                        for r in data {
                            process_request(&db, r.address, r.accounts, &dn_verifier).await?;
                        }
                    }
                    WatcherMessage::ActiveDisplayNames(data) => {
                        for mut d in data {
                            d.try_decode_hex();

                            let context = create_context(d.address);
                            let entry = DisplayNameEntry {
                                context,
                                display_name: d.display_name,
                            };

                            db.insert_display_name(&entry).await?;
                        }
                    }
                }

                Ok(())
            }.into_actor(self)
        )
    }
}

// Handle messages that should be sent to the Watcher.
impl Handler<ClientCommand> for Connector {
    type Result = ();

    fn handle(&mut self, msg: ClientCommand, _ctx: &mut Context<Self>) {
        match msg {
            ClientCommand::ProvideJudgement(id) => {
                let _ = self.sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::JudgementResult,
                        data: JudgementResponse {
                            address: id.address,
                            judgement: Judgement::Reasonable,
                        },
                    })
                    .unwrap()
                    .into(),
                ));
            }
            ClientCommand::RequestPendingJudgements => {
                let _ = self.sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::PendingJudgementsRequest,
                        data: (),
                    })
                    .unwrap()
                    .into(),
                ));
            }
            ClientCommand::RequestDisplayNames => {
                let _ = self.sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::DisplayNamesRequest,
                        data: (),
                    })
                    .unwrap()
                    .into(),
                ));
            }
        }
    }
}

/// Handle websocket messages received from the Watcher.
impl StreamHandler<std::result::Result<Frame, WsProtocolError>> for Connector {
    fn handle(
        &mut self,
        msg: std::result::Result<Frame, WsProtocolError>,
        _ctx: &mut Context<Self>,
    ) {
        fn local(
            queue: &mut UnboundedSender<WatcherMessage>,
            msg: std::result::Result<Frame, WsProtocolError>,
        ) -> Result<()> {
            let parsed: ResponseMessage<serde_json::Value> = match msg {
<<<<<<< HEAD
                Ok(Frame::Text(bytes)) | Ok(Frame::Binary(bytes)) => serde_json::from_slice(&bytes)?,
=======
                Ok(Frame::Text(txt)) => serde_json::from_slice(&txt)?,
                Ok(Frame::Pong(_)) => return Ok(()),
                //_ => return Err(anyhow!("invalid message, expected text: {:?}", msg)),
>>>>>>> 0a3fcce (skip log on invalid message)
                _ => return Ok(()),
            };

            match parsed.event {
                EventType::Ack => {
                    let data: AckResponse = serde_json::from_value(parsed.data)?;
                    debug!("Received acknowledgement from Watcher: {:?}", data);

                    queue.send(WatcherMessage::Ack(data))?;
                }
                EventType::Error => {
                    error!("Received error from Watcher: {:?}", parsed.data);
                }
                EventType::NewJudgementRequest => {
                    let data: JudgementRequest = serde_json::from_value(parsed.data)?;
                    debug!("Received new judgement request from Watcher: {:?}", data);

                    queue.send(WatcherMessage::NewJudgementRequest(data))?
                }
                EventType::PendingJudgementsResponse => {
                    let data: Vec<JudgementRequest> = serde_json::from_value(parsed.data)?;
                    debug!("Received pending judgments from Watcher: {:?}", data);

                    queue.send(WatcherMessage::PendingJudgementsRequests(data))?
                }
                EventType::DisplayNamesResponse => {
                    let data: Vec<DisplayNameEntryRaw> = serde_json::from_value(parsed.data)?;
                    debug!("Received Display Names");

                    queue.send(WatcherMessage::ActiveDisplayNames(data))?
                }
                EventType::JudgementUnrequested => {
                    debug!("Judgement unrequested (NOT SUPPORTED): {:?}", parsed);
                }
                _ => {
                    warn!("Received unrecognized message from watcher: {:?}", parsed);
                }
            }

            Ok(())
        }

        if let Err(err) = local(&mut self.queue, msg) {
            error!("Failed to process message in websocket stream: {:?}", err);
        }
    }

    fn started(&mut self, _ctx: &mut Context<Self>) {
        info!("Started stream handler for connector");
    }

    /// If connection dropped, try to reconnect.
    fn finished(&mut self, ctx: &mut Context<Self>) {
        warn!("Watcher disconnected");

        let db = self.db.clone();
        let dn_verifier = self.dn_verifier.clone();
        actix::spawn(async move {
            loop {
                // TODO: Handle
                let n = Connector::start("TODO", db.clone(), dn_verifier.clone()).await;
            }
        });

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
            AccountType::Twitter => IdentityFieldValue::Twitter(value.to_lowercase()),
            AccountType::Matrix => IdentityFieldValue::Matrix(value),
            AccountType::PGPFingerprint => IdentityFieldValue::PGPFingerprint(()),
            AccountType::Image => IdentityFieldValue::Image(()),
            AccountType::Additional => IdentityFieldValue::Additional(()),
        }
    }
}
