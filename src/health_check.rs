use crate::primitives::Result;
use actix_web::http::StatusCode;
use actix_web::{get, rt, App, HttpServer, Responder};

/// Currently, the health check endpoint just returns a "200 OK" response. In
/// the future, this service might be improved for more advanced reporting.
pub struct HealthCheck {}

#[get("/healthcheck")]
async fn endpoint() -> impl Responder {
    "OK".with_status(StatusCode::OK)
}

impl HealthCheck {
    pub fn start() -> Result<()> {
        let mut sys = rt::System::new("health check service");

        let server = HttpServer::new(|| App::new().service(endpoint))
            .bind("127.0.0.1:8080")?
            .run();

        sys.block_on(server)?;

        Ok(())
    }
}
