use crate::manager::NetworkAddress;
use actix::prelude::*;
use actix_web_actors::ws;
use actix_broker::BrokerIssue;

struct WsAccountStatusSession;

impl Actor for WsAccountStatusSession {
    type Context = ws::WebsocketContext<Self>;
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsAccountStatusSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        let msg = if let Ok(msg) = msg {
            msg
        } else {
            ctx.stop();
            return;
        };

        match msg {
            ws::Message::Text(txt) => {
                if let Ok(net_address) = serde_json::from_str::<NetworkAddress>(txt.as_str()) {
                    self.issue_system_async(SubscribeAccountStatus {
                        net_address: net_address,
                    });
                } else {
                    ctx.text("Invalid message type");
                }
            },
            ws::Message::Ping(b) => {
                ctx.pong(&b);
            }
            ws::Message::Close(reason) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
struct SubscribeAccountStatus {
    net_address: NetworkAddress
}
