use crate::display_name::DisplayNameVerifier;
use crate::primitives::{
    ChainAddress, ChainName, IdentityContext, IdentityFieldValue, JudgementState, Timestamp,
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
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::Instrument;

// In seconds
const HEARTBEAT_INTERVAL: u64 = 30;
#[cfg(not(test))]
const PENDING_JUDGEMENTS_INTERVAL: u64 = 10;
#[cfg(not(test))]
const DISPLAY_NAMES_INTERVAL: u64 = 10;
#[cfg(not(test))]
const JUDGEMENT_CANDIDATES_INTERVAL: u64 = 10;

#[cfg(test)]
const PENDING_JUDGEMENTS_INTERVAL: u64 = 1;
#[cfg(test)]
const DISPLAY_NAMES_INTERVAL: u64 = 1;
#[cfg(test)]
const JUDGEMENT_CANDIDATES_INTERVAL: u64 = 1;

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
        let span = info_span!("connector_initialization");
        span.in_scope(|| {
            debug!(
                network = config.network.as_str(),
                endpoint = config.endpoint.as_str()
            );
        });

        async {
            // Start Connector.
            let dn_verifier = DisplayNameVerifier::new(db.clone(), dn_config.clone());
            let conn =
                Connector::start(config.endpoint, config.network, db.clone(), dn_verifier).await?;

            info!("Connection initiated");
            info!("Sending pending judgements request to Watcher");
            let _ = conn.send(ClientCommand::RequestPendingJudgements).await?;

            Result::Ok(())
        }
        .instrument(span)
        .await?;
    }

    Ok(())
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
pub struct JudgementRequest {
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
pub enum WatcherMessage {
    Ack(AckResponse),
    NewJudgementRequest(JudgementRequest),
    PendingJudgementsRequests(Vec<JudgementRequest>),
    ActiveDisplayNames(Vec<DisplayNameEntryRaw>),
}

// TODO: Rename
#[derive(Debug, Clone, Message)]
#[rtype(result = "crate::Result<()>")]
pub enum ClientCommand {
    ProvideJudgement(IdentityContext),
    RequestPendingJudgements,
    RequestDisplayNames,
    Ping,
}

/// Handles incoming and outgoing websocket messages to and from the Watcher.
struct Connector {
    #[allow(clippy::type_complexity)]
    sink: Option<SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>>,
    db: Database,
    dn_verifier: DisplayNameVerifier,
    endpoint: String,
    network: ChainName,
    outgoing: UnboundedSender<ClientCommand>,
    inserted_states: Arc<RwLock<Vec<JudgementState>>>,
    // Tracks the last message received from the Watcher. If a certain treshold
    // was exceeded, the Connector attempts to reconnect.
    last_watcher_msg: Timestamp,
}

impl Connector {
    async fn start(
        endpoint: String,
        network: ChainName,
        db: Database,
        dn_verifier: DisplayNameVerifier,
    ) -> Result<Addr<Connector>> {
        let (_, framed) = Client::new()
            .ws(&endpoint)
            .max_frame_size(5_000_000)
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
                endpoint,
                network,
                outgoing,
                inserted_states: Default::default(),
                last_watcher_msg: Timestamp::now(),
            }
        });

        Ok(actor)
    }
    // Send a heartbeat to the Watcher every couple of seconds.
    #[allow(unused)]
    fn start_heartbeat_task(&self, ctx: &mut Context<Self>) {
        info!("Starting heartbeat background task");

        ctx.run_interval(Duration::new(HEARTBEAT_INTERVAL, 0), |_act, ctx| {
            ctx.address().do_send(ClientCommand::Ping)
        });
    }
    // Process any tangling submissions, meaning any verified requests that were
    // submitted to the Watcher but the issued extrinsic was not direclty
    // confirmed back. This usually does not happen, but can.
    fn start_dangling_judgements_task(&self, ctx: &mut Context<Self>) {
        info!("Starting dangling judgement pruning task");

        ctx.run_interval(Duration::new(60, 0), |act, _ctx| {
            let db = act.db.clone();

            actix::spawn(async move {
                if let Err(err) = db.process_dangling_judgement_states().await {
                    error!("Error when pruning dangling judgements: {:?}", err);
                }
            });
        });
    }
    // Request pending judgements every couple of seconds.
    fn start_pending_judgements_task(&self, ctx: &mut Context<Self>) {
        info!("Starting pending judgement requester background task");

        ctx.run_interval(
            Duration::new(PENDING_JUDGEMENTS_INTERVAL, 0),
            |_act, ctx| {
                ctx.address()
                    .do_send(ClientCommand::RequestPendingJudgements)
            },
        );
    }
    // Request actively used display names every couple of seconds.
    fn start_active_display_names_task(&self, ctx: &mut Context<Self>) {
        info!("Starting display name requester background task");

        ctx.run_interval(Duration::new(DISPLAY_NAMES_INTERVAL, 0), |_act, ctx| {
            ctx.address().do_send(ClientCommand::RequestDisplayNames)
        });
    }
    // Look for verified identities and submit those to the Watcher.
    fn start_judgement_candidates_task(&self, ctx: &mut Context<Self>) {
        info!("Starting judgement candidate submitter background task");

        let db = self.db.clone();
        let addr = ctx.address();
        let network = self.network;

        ctx.run_interval(
            Duration::new(JUDGEMENT_CANDIDATES_INTERVAL, 0),
            move |_act, _ctx| {
                let db = db.clone();
                let addr = addr.clone();

                actix::spawn(async move {
                    // Provide judgments.
                    // TODO: Should accept network parameter.
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
            },
        );
    }
}

impl Actor for Connector {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        let span = info_span!("connector_background_tasks");

        span.in_scope(|| {
            debug!(
                network = self.network.as_str(),
                endpoint = self.endpoint.as_str()
            );

            // Note: heartbeat task remains disabled.
            //self.start_heartbeat_task(ctx);
            self.start_pending_judgements_task(ctx);
            self.start_dangling_judgements_task(ctx);
            self.start_active_display_names_task(ctx);
            self.start_judgement_candidates_task(ctx);
        });
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        let span = warn_span!("watcher_connection_drop");
        span.in_scope(|| {
            debug!(
                network = self.network.as_str(),
                endpoint = self.endpoint.as_str()
            );
        });

        let endpoint = self.endpoint.clone();
        let network = self.network;
        let db = self.db.clone();
        let dn_verifier = self.dn_verifier.clone();

        actix::spawn(
            async move {
                warn!("Watcher disconnected, trying to reconnect...");

                let mut counter = 0;
                loop {
                    if Connector::start(endpoint.clone(), network, db.clone(), dn_verifier.clone())
                        .await
                        .is_err()
                    {
                        warn!("Reconnection failed, retrying...");

                        counter += 1;
                        if counter >= 10 {
                            error!("Cannot reconnect to Watcher after {} attempts", counter);
                        }

                        sleep(Duration::from_secs(10)).await;
                    } else {
                        info!("Reconnected to Watcher!");
                        break;
                    }
                }
            }
            .instrument(span),
        );
    }
}

impl WriteHandler<WsProtocolError> for Connector {}

// Handle messages that should be sent to the Watcher.
impl Handler<ClientCommand> for Connector {
    type Result = crate::Result<()>;

    fn handle(&mut self, msg: ClientCommand, ctx: &mut Context<Self>) -> Self::Result {
        let span = debug_span!("handling_client_message");

        // NOTE: make sure no async code comes after this.
        let _guard = span.enter();
        debug!(
            network = self.network.as_str(),
            endpoint = self.endpoint.as_str()
        );

        // If the sink (outgoing WS stream) is not configured (i.e. when
        // testing), send the client command to the channel.
        if self.sink.is_none() {
            warn!("Skipping message to Watcher, not configured (only occurs when testing)");
            self.outgoing.send(msg).unwrap();
            return Ok(());
        }

        let sink = self.sink.as_mut().unwrap();

        // Do a connection check and reconnect if necessary.
        if sink.closed() {
            ctx.stop();
            return Ok(());
        }

        // Do a timestamp check and reconnect if necessary.
        if Timestamp::now().raw() - self.last_watcher_msg.raw() > (HEARTBEAT_INTERVAL * 2) {
            warn!("Last received message from the Watcher was a while ago, resetting connection");
            ctx.stop();
            return Ok(());
        }

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
            id: IdentityContext,
            mut accounts: HashMap<AccountType, String>,
            dn_verifier: &DisplayNameVerifier,
            inserted_states: &Arc<RwLock<Vec<JudgementState>>>,
        ) -> Result<()> {
            // Decode display name if appropriate.
            if let Some((_, val)) = accounts
                .iter_mut()
                .find(|(ty, _)| *ty == &AccountType::DisplayName)
            {
                try_decode_hex(val);
            }

            let state = JudgementState::new(id, accounts.into_iter().map(|a| a.into()).collect());

            // Add the judgement state that's about to get inserted into the
            // local queue which is then fetched from the unit tests.
            #[cfg(not(test))]
            let _ = inserted_states;
            #[cfg(test)]
            {
                let mut l = inserted_states.write().await;
                (*l).push(state.clone());
            }

            db.add_judgement_request(&state).await?;
            dn_verifier.verify_display_name(&state).await?;

            Ok(())
        }

        // Update timestamp
        self.last_watcher_msg = Timestamp::now();

        let network = self.network;
        let db = self.db.clone();
        let dn_verifier = self.dn_verifier.clone();
        let inserted_states = Arc::clone(&self.inserted_states);

        Box::pin(
            async move {
                match msg {
                    WatcherMessage::Ack(data) => {
                        if data.result.to_lowercase().contains("judgement given") {
                            // Create identity context.
                            let address =
                                data.address
                                .ok_or_else(|| {
                                anyhow!(
                                    "no address specified in 'judgement given' response from Watcher"
                                )
                            })?;

                            let context = IdentityContext::new(address, network);

                            info!("Marking {:?} as judged", context);
                            db.set_judged(&context).await?;
                        }
                    }
                    WatcherMessage::NewJudgementRequest(data) => {
                        let id = IdentityContext::new(data.address, network);
                        process_request(&db, id, data.accounts, &dn_verifier, &inserted_states).await?;
                    }
                    WatcherMessage::PendingJudgementsRequests(data) => {
                        // Convert data.
                        let data: Vec<(IdentityContext, HashMap<AccountType, String>)> = data
                            .into_iter()
                            .map(|req| (
                                IdentityContext::new(req.address, network),
                                req.accounts
                            ))
                            .collect();

                        for (context, accounts) in data {
                            process_request(&db, context, accounts, &dn_verifier, &inserted_states).await?;
                        }
                    }
                    WatcherMessage::ActiveDisplayNames(data) => {
                        for mut name in data {
                            name.try_decode_hex();

                            let context = IdentityContext::new(name.address, network);
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
                Ok(other) => {
                    debug!("Received unexpected message: {:?}", other);
                    return Ok(());
                }
                Err(err) => return Err(anyhow!("error message: {:?}", err)),
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

        let span = debug_span!("handling_websocket_message");
        span.in_scope(|| {
            debug!(
                network = self.network.as_str(),
                endpoint = self.endpoint.as_str()
            );

            let addr = ctx.address();
            actix::spawn(
                async move {
                    if let Err(err) = local(addr, msg).await {
                        error!("Failed to process message in websocket stream: {:?}", err);
                    }
                }
                .in_current_span(),
            );
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
pub mod tests {
    use super::*;
    use crate::{Database, DisplayNameConfig};
    use tokio::sync::mpsc::UnboundedReceiver;

    impl JudgementRequest {
        pub fn alice() -> Self {
            JudgementRequest {
                address: "1a2YiGNu1UUhJtihq8961c7FZtWGQuWDVMWTNBKJdmpGhZP"
                    .to_string()
                    .into(),
                accounts: HashMap::from([
                    (AccountType::DisplayName, "Alice".to_string()),
                    (AccountType::Email, "alice@email.com".to_string()),
                    (AccountType::Twitter, "@alice".to_string()),
                    (AccountType::Matrix, "@alice:matrix.org".to_string()),
                ]),
            }
        }
        pub fn bob() -> Self {
            JudgementRequest {
                address: "1b3NhsSEqWSQwS6nPGKgCrSjv9Kp13CnhraLV5Coyd8ooXB"
                    .to_string()
                    .into(),
                accounts: HashMap::from([
                    (AccountType::DisplayName, "Bob".to_string()),
                    (AccountType::Email, "bob@email.com".to_string()),
                    (AccountType::Twitter, "@bob".to_string()),
                    (AccountType::Matrix, "@bob:matrix.org".to_string()),
                ]),
            }
        }
    }

    impl WatcherMessage {
        pub fn new_judgement_request(req: JudgementRequest) -> Self {
            WatcherMessage::NewJudgementRequest(req)
        }
    }

    pub struct ConnectorMocker {
        // Queue for outgoing messages to the Watcher (mocked).
        queue: UnboundedReceiver<ClientCommand>,
        addr: Addr<Connector>,
        inserted_states: Arc<RwLock<Vec<JudgementState>>>,
    }

    impl ConnectorMocker {
        pub fn new(db: Database) -> Self {
            let dn_config = DisplayNameConfig {
                enabled: false,
                limit: 0.85,
            };

            let dn_verifier = DisplayNameVerifier::new(db.clone(), dn_config);
            let (addr, queue, inserted_states) =
                Connector::start_testing(ChainName::Polkadot, db, dn_verifier);

            ConnectorMocker {
                queue,
                addr,
                inserted_states,
            }
        }
        /// A message received from the Watcher (mocked).
        pub async fn inject(&self, msg: WatcherMessage) {
            self.addr.send(msg).await.unwrap().unwrap();
            // Give some time to process.
            sleep(Duration::from_secs(3)).await;
        }
        pub async fn inserted_states(&self) -> Vec<JudgementState> {
            let mut states = self.inserted_states.write().await;
            std::mem::take(&mut states)
        }
        /// A list of messages that were sent to the Watcher (mocked).
        pub fn outgoing(&mut self) -> (Vec<ClientCommand>, OutgoingCounter) {
            let mut outgoing = vec![];
            let mut counter = OutgoingCounter::default();

            while let Ok(msg) = self.queue.try_recv() {
                match msg {
                    ClientCommand::ProvideJudgement(_) => counter.provide_judgement += 1,
                    ClientCommand::RequestPendingJudgements => {
                        counter.request_pending_judgements += 1
                    }
                    ClientCommand::RequestDisplayNames => counter.request_display_names += 1,
                    ClientCommand::Ping => counter.ping += 1,
                }

                outgoing.push(msg);
            }

            (outgoing, counter)
        }
    }

    #[derive(Default)]
    pub struct OutgoingCounter {
        pub provide_judgement: usize,
        pub request_pending_judgements: usize,
        pub request_display_names: usize,
        pub ping: usize,
    }

    impl Connector {
        fn start_testing(
            network: ChainName,
            db: Database,
            dn_verifier: DisplayNameVerifier,
        ) -> (
            Addr<Connector>,
            UnboundedReceiver<ClientCommand>,
            Arc<RwLock<Vec<JudgementState>>>,
        ) {
            let (outgoing, recv) = mpsc::unbounded_channel();

            let inserted_states = Default::default();

            // Start actor.
            let addr = Connector {
                sink: None,
                db,
                dn_verifier,
                endpoint: "".to_string(),
                network,
                outgoing,
                inserted_states: Arc::clone(&inserted_states),
                last_watcher_msg: Timestamp::now(),
            }
            .start();

            (addr, recv, inserted_states)
        }
    }
}
