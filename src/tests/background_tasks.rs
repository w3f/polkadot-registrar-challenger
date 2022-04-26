use super::*;
use crate::actors::api::ResponseAccountState;
use crate::actors::connector::WatcherMessage;
use crate::primitives::IdentityContext;
use futures::{FutureExt, SinkExt, StreamExt};
use tokio::time::{sleep, Duration};

#[actix::test]
async fn background_outgoing_watcher_messages() {
    let (_db, mut connector, _api, _inj) = new_env().await;

    // Wait until enough messages have been sent to the Watcher (mocked).
    sleep(Duration::from_secs(10)).await;

    let (_out, counter) = connector.outgoing();
    assert!(counter.provide_judgement == 0);
    assert!(counter.request_pending_judgements > 5);
    assert!(counter.request_display_names > 5);
    assert!(counter.ping == 0);
}

#[actix::test]
async fn process_dangling_judgement_states() {
    let (db, connector, mut api, _inj) = new_env().await;

    // Insert judgement requests.
    connector.inject(alice_judgement_request()).await;
    connector.inject(bob_judgement_request()).await;
    connector.inject(eve_judgement_request()).await;

    let states = connector.inserted_states().await;
    let mut alice = states[0].clone();
    let mut bob = states[1].clone();
    let mut eve = states[2].clone();

    db.set_fully_verified(&alice.context).await.unwrap();
    db.set_fully_verified(&bob.context).await.unwrap();
    db.set_fully_verified(&eve.context).await.unwrap();

    alice.is_fully_verified = true;
    bob.is_fully_verified = true;
    eve.is_fully_verified = true;

    #[allow(clippy::bool_assert_comparison)]
    {
        assert_eq!(alice.judgement_submitted, false);
        assert_eq!(bob.judgement_submitted, false);
        assert_eq!(eve.judgement_submitted, false);
    }

    // Check subscribed states.
    {
        let mut stream = api.ws_at("/api/account_status").await.unwrap();
        stream.send(IdentityContext::alice().to_ws()).await.unwrap();

        let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
        assert_eq!(
            resp,
            JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
        );
    }

    {
        let mut stream = api.ws_at("/api/account_status").await.unwrap();
        stream.send(IdentityContext::bob().to_ws()).await.unwrap();

        let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
        assert_eq!(
            resp,
            JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
        );
    }

    {
        let mut stream = api.ws_at("/api/account_status").await.unwrap();
        stream.send(IdentityContext::eve().to_ws()).await.unwrap();

        let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
        assert_eq!(
            resp,
            JsonResult::Ok(ResponseAccountState::with_no_notifications(eve.clone()))
        );
    }

    // The Watcher only reports Bob as pending (mocked), meaning that Alice and
    // Eve have been verified but the Challenger was not explicitly notifed
    // about that process. This should result in Alice and Eve being marked as
    // "judgement_submitted" in the database.
    connector
        .inject(WatcherMessage::new_pending_requests(vec![
            JudgementRequest::bob(),
        ]))
        .await;

    alice.judgement_submitted = true;
    bob.judgement_submitted = true;
    eve.judgement_submitted = true;

    // Check subscribed states.
    {
        let mut stream = api.ws_at("/api/account_status").await.unwrap();
        stream.send(IdentityContext::alice().to_ws()).await.unwrap();

        let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
        assert_eq!(
            resp,
            JsonResult::Ok(ResponseAccountState::with_no_notifications(alice.clone()))
        );
    }

    {
        let mut stream = api.ws_at("/api/account_status").await.unwrap();
        stream.send(IdentityContext::bob().to_ws()).await.unwrap();

        let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
        assert_eq!(
            resp,
            JsonResult::Ok(ResponseAccountState::with_no_notifications(bob.clone()))
        );
    }

    {
        let mut stream = api.ws_at("/api/account_status").await.unwrap();
        stream.send(IdentityContext::eve().to_ws()).await.unwrap();

        let resp: JsonResult<ResponseAccountState> = stream.next().await.into();
        assert_eq!(
            resp,
            JsonResult::Ok(ResponseAccountState::with_no_notifications(eve.clone()))
        );
    }
}
