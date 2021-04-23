use crate::Result;
use crate::api_v2::session::WsAccountStatusSession;
use actix_web::{web, App, Error as ActixError, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

pub async fn run_rest_api_server_blocking(addr: &str) -> Result<()> {
    async fn account_status_server_route(
        req: HttpRequest,
        stream: web::Payload,
    ) -> std::result::Result<HttpResponse, ActixError> {
        ws::start(WsAccountStatusSession::default(), &req, stream)
    }

    let server = HttpServer::new(move || {
        App::new().service(web::resource("/api/account_status").to(account_status_server_route))
    })
    .bind(addr)?;

    server.run();
    Ok(())
}

#[test]
fn server() {
    let mut system = actix::System::new("");

    system.block_on(async {
        run_rest_api_server_blocking("localhost:8080").await;
    });

    system.run();
}
