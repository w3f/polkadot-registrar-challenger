use crate::adapters::tests::MessageInjector;
use crate::adapters::AdapterListener;
use crate::database::Database;
use crate::primitives::{
    ExpectedMessage, ExternalMessage, ExternalMessageType, JudgementState, MessageId, Timestamp,
};
use crate::tests::F;
use crate::{config_session_notifier, DatabaseConfig, DisplayNameConfig, NotifierConfig, Result};
use rand::{thread_rng, Rng};
use tokio::time::{sleep, Duration};

#[actix::test]
#[ignore]
async fn run_mocker() -> Result<()> {
    // Init logger
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_env_filter("system")
        .init();

    let mut rng = thread_rng();

    let db_config = DatabaseConfig {
        uri: "mongodb://localhost:27017".to_string(),
        name: format!("registrar_test_{}", rng.gen_range(u32::MIN..u32::MAX)),
    };

    let notifier_config = NotifierConfig {
        api_address: "localhost:8888".to_string(),
        display_name: DisplayNameConfig {
            enabled: true,
            limit: 0.85,
        },
    };

    info!("Starting mock adapter and session notifier instances");

    // Setup database
    let db = Database::new(&db_config.uri, &db_config.name).await?;

    config_session_notifier(db.clone(), notifier_config).await?;

    // Setup message verifier and injector.
    let injector = MessageInjector::new();
    let listener = AdapterListener::new(db.clone()).await;
    listener.start_message_adapter(injector.clone(), 1).await;

    info!("Mocker setup completed");

    let mut alice = JudgementState::alice();
    // Set display name to valid.
    *alice
        .get_field_mut(&F::ALICE_DISPLAY_NAME())
        .expected_display_name_check_mut()
        .0 = true;

    info!("INSERTING IDENTITY: Alice (1a2YiGNu1UUhJtihq8961c7FZtWGQuWDVMWTNBKJdmpGhZP)");
    db.add_judgement_request(&alice).await.unwrap();

    // Create messages and (valid/invalid) messages randomly.
    let mut rng = thread_rng();
    loop {
        let ty_msg: u32 = rng.gen_range(0..2);
        let ty_validity = rng.gen_range(0..1);
        let reset = rng.gen_range(0..9);

        if reset == 0 {
            warn!("Resetting Identity");
            db.delete_judgement(&alice.context).await.unwrap();

            alice = JudgementState::alice();
            // Set display name to valid.
            *alice
                .get_field_mut(&F::ALICE_DISPLAY_NAME())
                .expected_display_name_check_mut()
                .0 = true;

            db.add_judgement_request(&alice).await.unwrap();
        }

        let (origin, values) = match ty_msg {
            0 => {
                (ExternalMessageType::Email("alice@email.com".to_string()), {
                    // Get either valid or invalid message
                    match ty_validity {
                        0 => alice
                            .get_field(&F::ALICE_EMAIL())
                            .expected_message()
                            .to_message_parts(),
                        1 => ExpectedMessage::random().to_message_parts(),
                        _ => panic!(),
                    }
                })
            }
            1 => {
                (ExternalMessageType::Twitter("@alice".to_string()), {
                    // Get either valid or invalid message
                    match ty_validity {
                        0 => alice
                            .get_field(&F::ALICE_TWITTER())
                            .expected_message()
                            .to_message_parts(),
                        1 => ExpectedMessage::random().to_message_parts(),
                        _ => panic!(),
                    }
                })
            }
            2 => {
                (
                    ExternalMessageType::Matrix("@alice:matrix.org".to_string()),
                    {
                        // Get either valid or invalid message
                        match ty_validity {
                            0 => alice
                                .get_field(&F::ALICE_MATRIX())
                                .expected_message()
                                .to_message_parts(),
                            1 => ExpectedMessage::random().to_message_parts(),
                            _ => panic!(),
                        }
                    },
                )
            }
            _ => panic!(),
        };

        // Inject message.
        injector
            .send(ExternalMessage {
                origin,
                id: MessageId::from(0u32),
                timestamp: Timestamp::now(),
                values,
            })
            .await;

        sleep(Duration::from_secs(4)).await;
    }
}
