use self::judgement_state::WsAccountStatusSession;
use crate::database::Database;
use crate::{NotifierConfig, Result};
use actix::prelude::*;
use actix::registry::SystemRegistry;
use actix_cors::Cors;
use actix_web::{http, web, App, Error as ActixError, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use display_name_check::{check_display_name, DisplayNameChecker};
use second_challenge::{verify_second_challenge, SecondChallengeVerifier};

mod display_name_check;
mod judgement_state;
mod second_challenge;

// Reexport
pub use self::judgement_state::{LookupServer, NotifyAccountState, ResponseAccountState};
pub use self::second_challenge::VerifyChallenge;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Message)]
#[serde(rename_all = "snake_case", tag = "type", content = "message")]
#[rtype(result = "()")]
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
    let api_address = config.api_address.clone();

    // Add configured actor to the registry.
    let actor = LookupServer::new(db.clone()).start();
    SystemRegistry::set(actor.clone());
    SystemRegistry::set(SecondChallengeVerifier::new(db.clone()).start());
    SystemRegistry::set(DisplayNameChecker::new(db, config.display_name.clone()).start());

    // Run the WS server.
    let server = HttpServer::new(move || {
        // Setup CORS
        let mut cors = Cors::default()
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::ACCEPT,
                http::header::CONTENT_TYPE,
            ])
            .max_age(3600);

        // Allow each specified domain.
        for origin in &config.cors_allow_origin {
            cors = cors.allowed_origin(origin.as_str());
        }

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
    .bind(api_address.as_str())?;

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
    pub async fn run_test_server(db: Database) -> (TestServer, Addr<LookupServer>) {
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
