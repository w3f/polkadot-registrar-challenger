use super::Result;
use websocket::r#async::Handle;
use websocket::server::r#async::Server;
use websocket::server::NoTlsAcceptor;

pub struct Listener {
    server: Server<NoTlsAcceptor>,
}

impl Listener {
    pub fn new() -> Result<Self> {
        Ok(Listener {
            server: Server::bind("127.0.0.1:7001", &Handle::default())?,
        })
    }
    pub async fn start(self) {
        //self.server.incoming()
    }
}
