use crate::Result;
use actix::io::SinkWrite;
use actix::*;
use actix_codec::Framed;
use awc::{
    error::WsProtocolError,
    ws::{Codec, Frame, Message},
    BoxedSocket, Client,
};
use actix::io::WriteHandler;
use futures::stream::{SplitSink, StreamExt};
use std::time::Duration;

async fn create_connector(endpoint: &str) -> Result<Addr<Connector>> {
    let (_, framed) = Client::new()
        .ws(endpoint)
        .connect()
        .await
        .map_err(|err| anyhow!("failed to initiate client connector to {}: {:?}", endpoint, err))?;

    let (sink, stream) = framed.split();
    let actor = Connector::create(|ctx| {
        Connector::add_stream(stream, ctx);
        Connector(SinkWrite::new(sink, ctx))
    });

    Ok(actor)
}

struct Connector(SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>);

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
