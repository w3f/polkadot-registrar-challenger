use super::*;
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

// TODO: Test tangling judgements
