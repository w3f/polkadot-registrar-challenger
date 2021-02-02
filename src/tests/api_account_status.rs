use super::InMemEventStore;

#[tokio::test]
async fn submit_and_watch() {
    let _ = InMemEventStore::run();


}
