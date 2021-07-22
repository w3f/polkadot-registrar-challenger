use crate::primitives::{IdentityContext, Timestamp};
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
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;
use std::collections::HashMap;

type Cache = Arc<RwLock<HashMap<IdentityContext, Timestamp>>>;

async fn run_connector(db: Database, url: String) -> Result<()> {
    let cache = Default::default();
    let mut connector = init_connector(&db, url.as_str(), Arc::clone(&cache)).await?;

    actix::spawn(async move {
        async fn local(connector: &mut Addr<Connector>, db: &Database, url: &str, cache: Cache) -> Result<()> {
            // If connection dropped, try to reconnect
            if !connector.connected() {
                warn!("Connection to Watcher dropped, trying to reconnect...");
                *connector = init_connector(db, url, Arc::clone(&cache)).await?
            }

            let mut completed = db.fetch_completed().await?;
            {
                let lock = cache.read().await;
                completed.retain(|state| {
                    if let Some(timestamp) = lock.get(&state.context) {
                        if Timestamp::now().raw() - timestamp.raw() < 10 {
                            return false
                        }
                    }

                    true
                });
            }

            Ok(())
        }

        loop {
            match local(&mut connector, &db, url.as_str(), Arc::clone(&cache)).await {
                Ok(_) => {
                    sleep(Duration::from_secs(1)).await;
                }
                Err(err) => {
                    error!("Connector error: {:?}", err);
                    sleep(Duration::from_secs(10)).await;
                }
            } }
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

struct Connector {
    db: Database,
    sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    cache: Cache,
}

#[derive(Message)]
#[rtype(result = "()")]
enum ClientCommand {
    ProvideJudgement,
}

impl Handler<ClientCommand> for Connector {
    type Result = ();

    fn handle(&mut self, msg: ClientCommand, _ctx: &mut Context<Self>) {
        //self.sink.write(Message::Text(msg.0));
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
    fn handle(&mut self, msg: std::result::Result<Frame, WsProtocolError>, _: &mut Context<Self>) {
        if let Ok(Frame::Text(txt)) = msg {
            println!("Server: {:?}", txt)
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
