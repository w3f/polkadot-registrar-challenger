use crate::database::Database;

mod api_judgement_state;

async fn new_db() -> Database {
    use rand::{thread_rng, Rng};

    let random: [u8; 32] = thread_rng().gen();
    Database::new(
        "mongodb://localhost:27017/",
        &format!(
            "registrar_test_{}",
            String::from_utf8(random.to_vec()).unwrap()
        ),
    )
    .await
    .unwrap()
}
