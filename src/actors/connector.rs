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

    // Init processing queue.
    let (tx, recv) = unbounded_channel();

    // Run processing queue for incoming connector messages.
    info!("Starting processing queue for incoming messages");
    run_queue_processor(db.clone(), recv, dn_config).await;

    for config in watchers {
        info!("Initializing connection to Watcher: {:?}", config);

        let db = db.clone();
        let tx = tx.clone();

        let mut connector = init_connector(config.endpoint.as_str(), tx.clone()).await?;

        info!("Starting judgments event loop");
        actix::spawn(async move {
            async fn local(
                connector: &mut Addr<Connector>,
                db: &Database,
                config: &WatcherConfig,
                tx: UnboundedSender<QueueMessage>,
            ) -> Result<()> {
                // If connection dropped, try to reconnect
                if !connector.connected() {
                    warn!("Connection to Watcher dropped, trying to reconnect...");
                    *connector = init_connector(config.endpoint.as_str(), tx).await?
                }

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
    }

    Ok(())
}

/// Convenience function for creating a full identity context when only the
/// address itself is present. Only supports Kusama and Polkadot for now.
pub fn create_context(address: ChainAddress) -> IdentityContext {
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

async fn run_queue_processor(
    db: Database,
    mut recv: UnboundedReceiver<QueueMessage>,
    dn_config: DisplayNameConfig,
) {
    async fn process_request(
        db: &Database,
        address: ChainAddress,
        mut accounts: HashMap<AccountType, String>,
        dn_verifier: &DisplayNameVerifier,
    ) -> Result<()> {
        let id = create_context(address);

        // Decode display name if appropriate.
        accounts
            .iter_mut()
            .find(|(ty, _)| *ty == &AccountType::DisplayName)
            .map(|(_, val)| {
                try_decode_hex(val);
            });

        let state = JudgementState::new(id, accounts.into_iter().map(|a| a.into()).collect());
        db.add_judgement_request(&state).await?;
        dn_verifier.verify_display_name(&state).await?;

        Ok(())
    }

    let dn_verifier = DisplayNameVerifier::new(db.clone(), dn_config);

    async fn local(
        db: &Database,
        recv: &mut UnboundedReceiver<QueueMessage>,
        dn_verifier: &DisplayNameVerifier,
    ) -> Result<()> {
        info!("Starting event loop for incoming messages");
        while let Some(message) = recv.recv().await {
            match message {
                QueueMessage::Ack(data) => {
                    // TODO: Check the "result"
                    if data.result == "judgement given" {
                        let context = create_context(data.address.ok_or(anyhow!(
                            "no address specified in 'judgement given' response from Watcher"
                        ))?);

                        info!("Marking {:?} as judged", context);
                        db.set_judged(&context).await?;
                    }
                }
                QueueMessage::NewJudgementRequest(data) => {
                    process_request(&db, data.address, data.accounts, &dn_verifier).await?
                }
                QueueMessage::PendingJudgementsRequests(data) => {
                    for r in data {
                        process_request(&db, r.address, r.accounts, &dn_verifier).await?;
                    }
                }
                QueueMessage::ActiveDisplayNames(data) => {
                    for mut d in data {
                        d.try_decode_hex();

                        let context = create_context(d.address);
                        let entry = DisplayNameEntry {
                            context: context,
                            display_name: d.display_name,
                        };

                        db.insert_display_name(&entry).await?;
                    }
                }
            }
        }

        Ok(())
    }

    actix::spawn(async move {
        loop {
            let _ = local(&db, &mut recv, &dn_verifier)
                .await
                .map_err(|err| error!("Error in connector messaging queue: {:?}", err));

            // 10 second pause between errors.
            sleep(Duration::from_secs(10)).await;
        }
    });
}

async fn init_connector(
    endpoint: &str,
    queue: UnboundedSender<QueueMessage>,
) -> Result<Addr<Connector>> {
    let (_, framed) = Client::builder()
        .timeout(Duration::from_secs(120))
        .initial_window_size(1_000_000)
        .initial_connection_window_size(1_000_000)
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

#[derive(Debug, Clone)]
enum QueueMessage {
    Ack(AckResponse),
    NewJudgementRequest(JudgementRequest),
    PendingJudgementsRequests(Vec<JudgementRequest>),
    ActiveDisplayNames(Vec<DisplayNameEntryRaw>),
}

struct Connector {
    sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    queue: UnboundedSender<QueueMessage>,
}

#[derive(Message)]
#[rtype(result = "()")]
enum ClientCommand {
    ProvideJudgement(IdentityContext),
    RequestPendingJudgements,
    RequestDisplayNames,
}

impl Handler<ClientCommand> for Connector {
    type Result = ();

    fn handle(&mut self, msg: ClientCommand, _ctx: &mut Context<Self>) {
        match msg {
            ClientCommand::ProvideJudgement(id) => {
                let _ = self.sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::JudgementResult,
                        data: JudgementResponse {
                            address: id.address.clone(),
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
            let _ = act
                .sink
                .write(Message::Ping(String::from("").into()))
                .map(|_| act.heartbeat(ctx))
                .map_err(|err| error!("Failed to send heartbeat to Watcher: {:?}", err));

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
        fn local(
            queue: &mut UnboundedSender<QueueMessage>,
            msg: std::result::Result<Frame, WsProtocolError>,
        ) -> Result<()> {
            let parsed: ResponseMessage<serde_json::Value> = match msg {
                Ok(Frame::Text(bytes)) | Ok(Frame::Binary(bytes)) => serde_json::from_slice(&bytes)?,
                _ => return Ok(()),
            };

            match parsed.event {
                EventType::Ack => {
                    let data: AckResponse = serde_json::from_value(parsed.data)?;
                    debug!("Received acknowledgement from Watcher: {:?}", data);

                    queue.send(QueueMessage::Ack(data))?;
                }
                EventType::Error => {
                    error!("Received error from Watcher: {:?}", parsed.data);
                }
                EventType::NewJudgementRequest => {
                    let data: JudgementRequest = serde_json::from_value(parsed.data)?;
                    debug!("Received new judgement request from Watcher: {:?}", data);

                    queue.send(QueueMessage::NewJudgementRequest(data))?
                }
                EventType::PendingJudgementsResponse => {
                    let data: Vec<JudgementRequest> = serde_json::from_value(parsed.data)?;
                    debug!("Received pending judgments from Watcher: {:?}", data);

                    queue.send(QueueMessage::PendingJudgementsRequests(data))?
                }
                EventType::DisplayNamesResponse => {
                    let data: Vec<DisplayNameEntryRaw> = serde_json::from_value(parsed.data)?;
                    debug!("Received Display Names");

                    queue.send(QueueMessage::ActiveDisplayNames(data))?
                }
                EventType::JudgementUnrequested => {
                    debug!("Judgement unrequested: {:?}", parsed);
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
            AccountType::Twitter => IdentityFieldValue::Twitter(value.to_lowercase()),
            AccountType::Matrix => IdentityFieldValue::Matrix(value),
            AccountType::PGPFingerprint => IdentityFieldValue::PGPFingerprint(()),
            AccountType::Image => IdentityFieldValue::Image(()),
            AccountType::Additional => IdentityFieldValue::Additional(()),
        }
    }
}
