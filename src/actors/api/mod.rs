use self::judgement_state::{LookupServer, WsAccountStatusSession};
use crate::database::Database;
use crate::Result;
use actix::prelude::*;
use actix::registry::SystemRegistry;
use actix_web::{web, App, Error as ActixError, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;

mod judgement_state;

// Reexport
pub use self::judgement_state::NotifyAccountState;

#[derive(Debug, Clone, Serialize, Message)]
#[serde(rename_all = "snake_case", tag = "result_type", content = "message")]
#[rtype(result = "()")]
// TODO: Rename this
pub enum JsonResult<T> {
    Ok(T),
    Err(String),
}

pub async fn run_rest_api_server_blocking(addr: &str, db: Database) -> Result<()> {
    async fn account_status_server_route(
        req: HttpRequest,
        stream: web::Payload,
    ) -> std::result::Result<HttpResponse, ActixError> {
        ws::start(WsAccountStatusSession::default(), &req, stream)
    }

    // Add configured actor to the registry.
    let actor = LookupServer::new(db).start();
    SystemRegistry::set(actor);

    // Run the WS server.
    let server = HttpServer::new(move || {
        App::new().service(web::resource("/api/account_status").to(account_status_server_route))
    })
    .bind(addr)?;

    server.run();
    Ok(())
}

/*
#[test]
fn server() {
    let mut system = actix::System::new("");

    system.block_on(async {
        run_rest_api_server_blocking("localhost:8080").await;
    });

    system.run();
}
*/
