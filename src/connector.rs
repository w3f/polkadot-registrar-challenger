use crate::primitives::{
    ChainAddress, ChainName, IdentityContext, IdentityFieldValue, JudgementState, Timestamp,
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
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

type Cache = Arc<RwLock<HashMap<ChainAddress, Timestamp>>>;

async fn run_connector(db: Database, url: String) -> Result<()> {
    let cache = Default::default();
    let mut connector = init_connector(&db, url.as_str(), Arc::clone(&cache)).await?;

    actix::spawn(async move {
        async fn local(
            connector: &mut Addr<Connector>,
            db: &Database,
            url: &str,
            cache: Cache,
        ) -> Result<()> {
            // If connection dropped, try to reconnect
            if !connector.connected() {
                warn!("Connection to Watcher dropped, trying to reconnect...");
                *connector = init_connector(db, url, Arc::clone(&cache)).await?
            }

            let mut completed = db.fetch_completed().await?;
            {
                let lock = cache.read().await;
                completed.retain(|state| {
                    if let Some(timestamp) = lock.get(&state.context.address) {
                        if Timestamp::now().raw() - timestamp.raw() < 10 {
                            return false;
                        }
                    }

                    true
                });
            }

            for state in completed {
                connector.do_send(ClientCommand::ProvideJudgement(state.context));
            }

            Ok(())
        }

        loop {
            match local(&mut connector, &db, url.as_str(), Arc::clone(&cache)).await {
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

async fn init_connector(db: &Database, endpoint: &str, cache: Cache) -> Result<Addr<Connector>> {
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
            db: db.clone(),
            sink: SinkWrite::new(sink, ctx),
            cache: cache,
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

struct Connector {
    db: Database,
    sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    cache: Cache,
}

#[derive(Message)]
#[rtype(result = "()")]
enum ClientCommand {
    ProvideJudgement(IdentityContext),
}

impl Handler<ClientCommand> for Connector {
    type Result = ();

    fn handle(&mut self, msg: ClientCommand, ctx: &mut Context<Self>) {
        let mut address = None;
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

                address = Some(id.address);
            }
        }

        if let Some(address) = address {
            let cache = Arc::clone(&self.cache);
            ctx.spawn(
                async move {
                    let mut lock = cache.write().await;
                    lock.insert(address, Timestamp::now());
                }
                .into_actor(self),
            );
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
            //act.0.write(Message::Ping(b""));
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
        ctx: &mut Context<Self>,
    ) {
        let parsed: ResponseMessage<serde_json::Value> = if let Ok(Frame::Text(txt)) = msg {
            serde_json::from_slice(&txt).unwrap()
        } else {
            return;
        };

        match parsed.event {
            EventType::Ack => {
                // TODO: Check the "result"
                let data: AckResponse = serde_json::from_value(parsed.data).unwrap();
                debug!("Received acknowledgement from Watcher: {:?}", data);

                let db = self.db.clone();
                let cache = Arc::clone(&self.cache);
                if let Some(address) = data.address {
                    ctx.spawn(
                        async move {
                            db.remove_judgement_request(&address).await.unwrap();
                            let mut lock = cache.write().await;
                            lock.remove(&address);
                        }
                        .into_actor(self),
                    );
                }
            }
            EventType::Error => {
                error!("Received error from Watcher: {:?}", parsed.data);
            }
            EventType::NewJudgementRequest => {
                let data: JudgementRequest = serde_json::from_value(parsed.data).unwrap();
                debug!("Received new judgement request from Watcher: {:?}", data);

                let db = self.db.clone();
                ctx.spawn(
                    async move {
                        process_request(&db, data.address, data.accounts)
                            .await
                            .unwrap();
                    }
                    .into_actor(self),
                );
            }
            EventType::PendingJudgementsResponse => {
                let requests: Vec<JudgementRequest> = serde_json::from_value(parsed.data).unwrap();
                debug!("Received pending judgments from Watcher: {:?}", requests);

                let db = self.db.clone();
                ctx.spawn(
                    async move {
                        for data in requests {
                            process_request(&db, data.address, data.accounts)
                                .await
                                .unwrap();
                        }
                    }
                    .into_actor(self),
                );
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

async fn process_request(
    db: &Database,
    address: ChainAddress,
    accounts: HashMap<AccountType, String>,
) -> Result<()> {
    let chain = if address.as_str().starts_with("1") {
        ChainName::Polkadot
    } else {
        ChainName::Kusama
    };

    let id = IdentityContext {
        address: address,
        chain: chain,
    };

    let state = JudgementState::new(id, accounts.into_iter().map(|a| a.into()).collect());

    db.add_judgement_request(state).await?;

    Ok(())
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
