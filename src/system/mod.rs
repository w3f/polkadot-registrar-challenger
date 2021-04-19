use crate::adapters::matrix::MatrixClient;
use crate::adapters::twitter::TwitterBuilder;
use crate::aggregate::verifier::{VerifierAggregate, VerifierAggregateId, VerifierCommand};
use crate::aggregate::{
    Aggregate, MessageWatcher, MessageWatcherCommand, MessageWatcherId, Repository,
};
use crate::api::{ConnectionPool, PublicRpc, PublicRpcApi};
use crate::api_v2::session::WsAccountStatusSession;
use crate::event::{Event, EventType, ExternalMessage};
use crate::projection::{Projector, SessionNotifier};
use crate::{adapters::email::SmtpImapClientBuilder, manager::IdentityManager};
use crate::{EmailConfig, MatrixConfig, Result, TwitterConfig};
use actix_web::{web, App, Error as ActixError, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use async_channel::Receiver;
use eventstore::Client;
use futures::join;
use futures::stream::StreamExt;
use jsonrpc_pubsub::{PubSubHandler, Session};
use jsonrpc_ws_server::{RequestContext, Server as WsServer, ServerBuilder};
use std::sync::Arc;
use tokio::time::{self, Duration};

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

/*
pub async fn run_verifier_subscription(
    client: Client,
) -> Result<()> {
    info!("Starting message subscription");

    let repository = Repository::new(VerifierAggregate.into(), store);

    subscription
        .resume()
        .await?
        .for_each(|persisted| async {
            let event: Event = if let Ok(persisted) = persisted {
                if let Ok(event) = persisted.take().as_json() {
                    event
                } else {
                    error!("Failed to parse event from stream");
                    return ();
                }
            } else {
                error!("Failed to acquire event from stream");
                return ();
            };

            let mut root = repository.get(VerifierAggregateId).await.unwrap();

            // Verify message with the aggregate.
            match event.body {
                EventType::ExternalMessage(message) => {
                    let _ = root
                        .handle(VerifierCommand::VerifyMessage(message))
                        .await
                        .map_err(|err| error!("Failed to verify message with aggregate: {}", err));
                }
                _ => warn!("Received unexpected message"),
            }

            ()
        })
        .await;

    Ok(())
}
*/

pub async fn run_matrix_listener_blocking(
    config: MatrixConfig,
    repo: Repository<MessageWatcher>,
) -> Result<()> {
    info!("Configuring Matrix client");

    let (client, recv) = MatrixClient::new(
        &config.homeserver,
        &config.username,
        &config.password,
        &config.db_path,
    )
    .await?;

    info!("Starting Matrix client");
    client.start().await;

    messages_event_loop(repo, recv, "email").await
}

pub async fn run_email_listener_blocking(
    config: EmailConfig,
    repo: Repository<MessageWatcher>,
) -> Result<()> {
    info!("Configuring email client");

    let (client, recv) = SmtpImapClientBuilder::new()
        .email_server(config.smtp_server)
        .imap_server(config.imap_server)
        .email_inbox(config.inbox)
        .email_user(config.user)
        .email_password(config.password)
        .request_interval(config.request_interval)
        .build()?;

    info!("Starting email client");
    client.start().await;

    messages_event_loop(repo, recv, "email").await
}

pub async fn run_twitter_listener_blocking(
    config: TwitterConfig,
    repo: Repository<MessageWatcher>,
) -> Result<()> {
    info!("Configuring Twitter client");

    let (mut client, recv) = TwitterBuilder::new()
        .consumer_key(config.api_key)
        .consumer_secret(config.api_secret)
        .token(config.token)
        .token_secret(config.token_secret)
        .request_interval(config.request_interval)
        .build()?;

    info!("Starting Twitter client");
    client.start().await;

    messages_event_loop(repo, recv, "Twitter").await
}

/// For each message received by an adapter, send a command to the aggregate and
/// let it handle it. This aggregate does not actually need to maintain a state.
async fn messages_event_loop<T>(
    mut repo: Repository<MessageWatcher>,
    recv: Receiver<T>,
    name: &str,
) -> Result<()>
where
    T: Into<ExternalMessage>,
{
    info!("Starting event loop for incoming {} messages", name);
    while let Ok(message) = recv.recv().await {
        repo.apply(MessageWatcherCommand::AddMessage(message.into()))
            .await?;
    }

    Err(anyhow!("The {} client has shut down", name))
}
