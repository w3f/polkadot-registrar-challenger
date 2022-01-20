use self::judgement_state::WsAccountStatusSession;
use crate::database::Database;
use crate::{NotifierConfig, Result};
use actix::prelude::*;
use actix::registry::SystemRegistry;
use actix_cors::Cors;
use actix_web::{web, App, Error as ActixError, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use display_name_check::{check_display_name, DisplayNameChecker};
use second_challenge::{verify_second_challenge, SecondChallengeVerifier};

mod display_name_check;
mod judgement_state;
mod second_challenge;

// Reexport
pub use self::judgement_state::{LookupServer, NotifyAccountState, ResponseAccountState};
pub use self::second_challenge::VerifyChallenge;

// TODO: Unify all "*_type" values as just "type". See JSON output.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Message)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
#[rtype(result = "()")]
// TODO: Rename this
pub enum JsonResult<T> {
    Ok(T),
    Err(String),
}

async fn healthcheck() -> HttpResponse {
    HttpResponse::Ok().body("OK")
}

pub async fn run_rest_api_server(
    config: NotifierConfig,
    db: Database,
) -> Result<Addr<LookupServer>> {
    // Add configured actor to the registry.
    let actor = LookupServer::new(db.clone()).start();
    SystemRegistry::set(actor.clone());
    SystemRegistry::set(SecondChallengeVerifier::new(db.clone()).start());
    SystemRegistry::set(DisplayNameChecker::new(db, config.display_name).start());

    // Run the WS server.
    let server = HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .route("/healthcheck", web::get().to(healthcheck))
            .service(web::resource("/api/account_status").to(account_status_server_route))
            .route(
                "/api/verify_second_challenge",
                web::post().to(verify_second_challenge),
            )
            .route(
                "/api/check_display_name",
                web::post().to(check_display_name),
            )
    })
    .bind(config.api_address.as_str())?;

    actix::spawn(async move {
        let _ = server.run().await;
    });

    Ok(actor)
}

async fn account_status_server_route(
    req: HttpRequest,
    stream: web::Payload,
) -> std::result::Result<HttpResponse, ActixError> {
    ws::start(WsAccountStatusSession::default(), &req, stream)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::database::Database;
    use crate::DisplayNameConfig;
    use actix_test::{start, TestServer};

    impl Default for DisplayNameConfig {
        fn default() -> Self {
            DisplayNameConfig {
                enabled: false,
                limit: 0.85,
            }
        }
    }

    #[cfg(test)]
    // TODO: Unify this with the `run_rest_api_server_blocking` function above.
    pub async fn run_test_server(db: Database) -> (TestServer, Addr<LookupServer>) {
        // TODO: Use Actor::create?
        let actor = LookupServer::new(db.clone()).start();

        let t_actor = actor.clone();
        let server = start(move || {
            // Add configured actor to the registry.
            SystemRegistry::set(t_actor.clone());
            SystemRegistry::set(SecondChallengeVerifier::new(db.clone()).start());
            SystemRegistry::set(
                DisplayNameChecker::new(db.clone(), DisplayNameConfig::default()).start(),
            );

            App::new()
                .service(web::resource("/api/account_status").to(account_status_server_route))
                .route(
                    "/api/verify_second_challenge",
                    web::post().to(verify_second_challenge),
                )
                .route(
                    "/api/check_display_name",
                    web::post().to(check_display_name),
                )
        });

        (server, actor)
    }
}
