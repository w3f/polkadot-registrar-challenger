use super::InMemBackend;

#[tokio::test]
async fn submit_and_watch() {
    let _ = InMemBackend::run();
}
