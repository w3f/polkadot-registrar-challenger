use crate::display_name::DisplayNameVerifier;
use crate::primitives::{
    ChainAddress, ChainName, IdentityContext, IdentityFieldValue, JudgementState,
};
use crate::{Database, DisplayNameConfig, Result, WatcherConfig};
use actix::io::SinkWrite;
use actix::io::WriteHandler;
use actix::prelude::*;
use actix_codec::Framed;
use awc::{
    error::WsProtocolError,
    ws::{Codec, Frame, Message},
    BoxedSocket, Client,
};
use futures::stream::{SplitSink, StreamExt};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::time::sleep;

// In seconds
const HEARTBEAT_INTERVAL: u64 = 30;
const PENDING_JUDGEMENTS_INTERVAL: u64 = 10;

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

        // Start Connector.
        let dn_verifier = DisplayNameVerifier::new(db.clone(), dn_config.clone());
        let conn = Connector::start("", config.network, db.clone(), dn_verifier).await?;

        info!("Sending pending judgements request to Watcher");
        let _ = conn.send(ClientCommand::RequestPendingJudgements).await?;
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

#[derive(Debug, Clone, Message)]
#[rtype(result = "crate::Result<()>")]
enum ClientCommand {
    ProvideJudgement(IdentityContext),
    RequestPendingJudgements,
    RequestDisplayNames,
    Ping,
}

/// Handles incoming and outgoing websocket messages to and from the Watcher.
struct Connector {
    sink: Option<SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>>,
    db: Database,
    dn_verifier: DisplayNameVerifier,
    network: ChainName,
    outgoing: UnboundedSender<ClientCommand>,
}

impl Connector {
    async fn start(
        endpoint: &str,
        network: ChainName,
        db: Database,
        dn_verifier: DisplayNameVerifier,
    ) -> Result<Addr<Connector>> {
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

        // Create throw-away channels (`outgoing` in `Connector` is only used in tests.)
        let (outgoing, _recv) = mpsc::unbounded_channel();

        // Start the Connector actor with the attached websocket stream.
        let (sink, stream) = framed.split();
        let actor = Connector::create(|ctx| {
            Connector::add_stream(stream, ctx);
            Connector {
                sink: Some(SinkWrite::new(sink, ctx)),
                db,
                dn_verifier,
                network,
                outgoing,
            }
        });

        Ok(actor)
    }
    // Send a heartbeat to the Watcher every couple of seconds.
    fn start_heartbeat_task(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(Duration::new(HEARTBEAT_INTERVAL, 0), |act, ctx| {
            ctx.address().do_send(ClientCommand::Ping)
        });
    }
    // Request pending judgements every couple of seconds.
    fn start_pending_judgements_task(&self, ctx: &mut Context<Self>) {
        ctx.run_interval(
            Duration::new(PENDING_JUDGEMENTS_INTERVAL, 0),
            |_act, ctx| {
                ctx.address()
                    .do_send(ClientCommand::RequestPendingJudgements)
            },
        );
    }
    // Look for verified identities and submit those to the Watcher.
    fn start_judgement_candidates_task(&self, ctx: &mut Context<Self>) {
        let db = self.db.clone();
        let addr = ctx.address();
        let network = self.network;

        ctx.run_interval(Duration::new(10, 0), move |_act, _ctx| {
            let db = db.clone();
            let addr = addr.clone();

            actix::spawn(async move {
                // Provide judgments.
                match db.fetch_judgement_candidates().await {
                    Ok(completed) => {
                        for state in completed {
                            if state.context.chain == network {
                                info!("Notifying Watcher about judgement: {:?}", state.context);
                                addr.do_send(ClientCommand::ProvideJudgement(state.context));
                            } else {
                                debug!(
                                    "Skipping judgement on connector assigned to {:?}: {:?}",
                                    network, state.context
                                );
                            }
                        }
                    }
                    Err(err) => {
                        error!("Failed to fetch judgement candidates: {:?}", err);
                    }
                }
            });
        });
    }
}

impl Actor for Connector {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.start_heartbeat_task(ctx);
        self.start_pending_judgements_task(ctx);
        self.start_judgement_candidates_task(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        error!("Connector disconnected");
    }
}

impl WriteHandler<WsProtocolError> for Connector {}

// Handle messages that should be sent to the Watcher.
impl Handler<ClientCommand> for Connector {
    type Result = crate::Result<()>;

    fn handle(&mut self, msg: ClientCommand, _ctx: &mut Context<Self>) -> Self::Result {
        // If the sink (outgoing WS stream) is not configured (i.e. when
        // testing), send the client command to the channel.
        if self.sink.is_none() {
            warn!("Skipping message to Watcher, not configured (only occurs when testing)");
            self.outgoing.send(msg).unwrap();
            return Ok(());
        }

        let sink = self.sink.as_mut().unwrap();

        match msg {
            ClientCommand::ProvideJudgement(id) => {
                debug!("Providing judgement over websocket stream: {:?}", id);

                sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::JudgementResult,
                        data: JudgementResponse {
                            address: id.address,
                            judgement: Judgement::Reasonable,
                        },
                    })
                    .unwrap()
                    .into(),
                ))
                .map_err(|err| anyhow!("failed to provide judgement: {:?}", err))?;
            }
            ClientCommand::RequestPendingJudgements => {
                debug!("Requesting pending judgements over websocket stream");

                sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::PendingJudgementsRequest,
                        data: (),
                    })
                    .unwrap()
                    .into(),
                ))
                .map_err(|err| anyhow!("failed to request pending judgements: {:?}", err))?;
            }
            ClientCommand::RequestDisplayNames => {
                debug!("Requesting display names over websocket stream");

                sink.write(Message::Text(
                    serde_json::to_string(&ResponseMessage {
                        event: EventType::DisplayNamesRequest,
                        data: (),
                    })
                    .unwrap()
                    .into(),
                ))
                .map_err(|err| anyhow!("failed to request display names: {:?}", err))?;
            }
            ClientCommand::Ping => {
                debug!("Sending ping to Watcher over websocket stream");

                sink.write(Message::Text("ping".to_string().into()))
                    .map_err(|err| anyhow!("failed to send ping over websocket: {:?}", err))?;
            }
        }

        Ok(())
    }
}

// Handle messages that were received from the Watcher.
impl Handler<WatcherMessage> for Connector {
    type Result = ResponseActFuture<Self, crate::Result<()>>;

    fn handle(&mut self, msg: WatcherMessage, _ctx: &mut Context<Self>) -> Self::Result {
        /// Handle a judgement request.
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
                        // usually does not happen, but can.
                        {
                            let addresses: Vec<&ChainAddress> =
                                data.iter().map(|state| &state.address).collect();

                            db.process_tangling_submissions(&addresses).await?;
                        }

                        for r in data {
                            process_request(&db, r.address, r.accounts, &dn_verifier).await?;
                        }
                    }
                    WatcherMessage::ActiveDisplayNames(data) => {
                        for mut name in data {
                            name.try_decode_hex();

                            let context = create_context(name.address);
                            let entry = DisplayNameEntry {
                                context,
                                display_name: name.display_name,
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

/// Handle websocket messages received from the Watcher. Those messages will be
/// forwarded to the `Handler<WatcherMessage>` implementation.
impl StreamHandler<std::result::Result<Frame, WsProtocolError>> for Connector {
    fn handle(
        &mut self,
        msg: std::result::Result<Frame, WsProtocolError>,
        ctx: &mut Context<Self>,
    ) {
        async fn local(
            conn: Addr<Connector>,
            msg: std::result::Result<Frame, WsProtocolError>,
        ) -> Result<()> {
            let parsed: ResponseMessage<serde_json::Value> = match msg {
                Ok(Frame::Text(txt)) => serde_json::from_slice(&txt)?,
                Ok(Frame::Pong(_)) => return Ok(()),
                // Just ingore what isn't recognized.
                _ => return Ok(()),
            };

            match parsed.event {
                EventType::Ack => {
                    debug!("Received acknowledgement from Watcher: {:?}", parsed.data);

                    let data: AckResponse = serde_json::from_value(parsed.data)?;
                    conn.send(WatcherMessage::Ack(data)).await??;
                }
                EventType::Error => {
                    error!("Received error from Watcher: {:?}", parsed.data);
                }
                EventType::NewJudgementRequest => {
                    debug!(
                        "Received new judgement request from Watcher: {:?}",
                        parsed.data
                    );

                    let data: JudgementRequest = serde_json::from_value(parsed.data)?;
                    conn.send(WatcherMessage::NewJudgementRequest(data))
                        .await??;
                }
                EventType::PendingJudgementsResponse => {
                    debug!("Received pending judgments from Watcher: {:?}", parsed.data);

                    let data: Vec<JudgementRequest> = serde_json::from_value(parsed.data)?;
                    conn.send(WatcherMessage::PendingJudgementsRequests(data))
                        .await??;
                }
                EventType::DisplayNamesResponse => {
                    debug!("Received display names from the Watcher");

                    let data: Vec<DisplayNameEntryRaw> = serde_json::from_value(parsed.data)?;
                    conn.send(WatcherMessage::ActiveDisplayNames(data))
                        .await??;
                }
                _ => {
                    warn!("Received unrecognized message from Watcher: {:?}", parsed);
                }
            }

            Ok(())
        }

        let addr = ctx.address();
        actix::spawn(async move {
            if let Err(err) = local(addr, msg).await {
                error!("Failed to process message in websocket stream: {:?}", err);
            }
        });
    }

    fn started(&mut self, _ctx: &mut Context<Self>) {
        info!("Started stream handler for connector");
    }

    /// If connection dropped, try to reconnect.
    fn finished(&mut self, ctx: &mut Context<Self>) {
        ctx.terminate();

        warn!("Watcher disconnected, trying to reconnect...");

        let network = self.network;
        let db = self.db.clone();
        let dn_verifier = self.dn_verifier.clone();

        actix::spawn(async move {
            let mut counter = 0;
            loop {
                if Connector::start("TODO", network, db.clone(), dn_verifier.clone())
                    .await
                    .is_err()
                {
                    warn!("Reconnection failed, retrying...");

                    counter += 1;
                    if counter == 10 {
                        error!("Cannot reconnect to Watcher after {} attempts", counter);
                    }

                    sleep(Duration::from_secs(10)).await;
                }

                info!("Reconnected to Watcher!");
            }
        });
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::UnboundedReceiver;

    impl Connector {
        async fn start_testing(
            network: ChainName,
            db: Database,
            dn_verifier: DisplayNameVerifier,
        ) -> (Self, UnboundedReceiver<ClientCommand>) {
            let (outgoing, recv) = mpsc::unbounded_channel();

            (
                Connector {
                    sink: None,
                    db,
                    dn_verifier,
                    network,
                    outgoing,
                },
                recv,
            )
        }
    }
}
