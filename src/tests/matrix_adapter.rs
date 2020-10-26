use super::mocks::*;
use super::{db_path, pause};
use crate::connector::{EventType, JudgementRequest, JudgementResponse, Message, AckResponse};
use crate::primitives::{Account, AccountType, Challenge, Judgement, NetAccount};
use crate::verifier::VerifierMessage;
use crate::{test_run, Database2};
use matrix_sdk::identifiers::{RoomId, UserId};
use schnorrkel::Keypair;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::runtime::Runtime;

#[test]
fn matrix_init_message() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        // Setup database and manager.
        let db = Database2::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager2::new());
        let (_, matrix_child) = manager.child();

        let my_user_id = UserId::try_from("@registrar:matrix.org").unwrap();
        let matrix_transport = MatrixMocker::new(matrix_child, my_user_id);

        // Starts tasks.
        let handlers = test_run(
            Arc::clone(&manager),
            db,
            matrix_transport,
            DummyTransport::new(),
            DummyTransport::new(),
        )
        .await
        .unwrap();

        let injector = handlers.reader.injector();

        // Generate events.
        let msg = serde_json::to_string(&Message {
            event: EventType::NewJudgementRequest,
            data: serde_json::to_value(&JudgementRequest {
                address: NetAccount::alice(),
                accounts: [(
                    AccountType::Matrix,
                    Some(Account::from("@alice:matrix.org")),
                )]
                .iter()
                .cloned()
                .collect(),
            })
            .unwrap(),
        })
        .unwrap();

        // Send new judgement request.
        injector.send_message(msg.clone()).await;
        pause().await;

        // Verify events.
        let events = manager.events().await;

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::DisplayNamesRequest,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                }
            })
        ));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::PendingJudgementsRequests,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                }
            })
        ));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Reader { message: msg })
        ));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::Ack,
                    data: serde_json::to_value(&AckResponse {
                        result: String::from("Message acknowledged"),
                    }
                    ).unwrap(),
                }
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: UserId::try_from("@alice:matrix.org").unwrap(),
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::SendMessage {
                room_id: RoomId::try_from("!17:matrix.org").unwrap(),
                message: VerifierMessageBlank::InitMessageWithContext,
            })
        ));
    });
}

#[test]
fn matrix_valid_signature_response() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        // Setup database and manager.
        let db = Database2::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager2::new());
        let (_, matrix_child) = manager.child();

        let my_user_id = UserId::try_from("@registrar:matrix.org").unwrap();
        let matrix_transport = MatrixMocker::new(matrix_child, my_user_id);

        // Starts tasks.
        let handlers = test_run(
            Arc::clone(&manager),
            db,
            matrix_transport,
            DummyTransport::new(),
            DummyTransport::new(),
        )
        .await
        .unwrap();

        let matrix = handlers.matrix;
        let injector = handlers.reader.injector();

        let keypair = Keypair::generate();

        // Generate events.
        let msg = serde_json::to_string(&Message {
            event: EventType::NewJudgementRequest,
            data: serde_json::to_value(&JudgementRequest {
                address: NetAccount::from(&keypair.public),
                accounts: [(
                    AccountType::Matrix,
                    Some(Account::from("@alice:matrix.org")),
                )]
                .iter()
                .cloned()
                .collect(),
            })
            .unwrap(),
        })
        .unwrap();

        // Send new judgement request.
        injector.send_message(msg.clone()).await;
        pause().await;

        // Respond with valid signature.
        let signature =
            keypair.sign_simple(b"substrate", Challenge::gen_fixed().as_str().as_bytes());

        let room_id = RoomId::try_from("!17:matrix.org").unwrap();
        matrix.trigger_matrix_emitter(
            room_id.clone(),
            UserId::try_from("@registrar:matrix.org").unwrap(),
            MatrixEventMock {
                user_id: UserId::try_from("@alice:matrix.org").unwrap(),
                message: hex::encode(signature.to_bytes()),
            },
        );

        pause().await;

        // Verify events.
        let events = manager.events().await;

        // Skip startup events...

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: UserId::try_from("@alice:matrix.org").unwrap(),
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::SendMessage {
                room_id: room_id.clone(),
                message: VerifierMessageBlank::InitMessageWithContext
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::SendMessage {
                room_id: room_id.clone(),
                message: VerifierMessageBlank::ResponseValid,
            })
        ));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::JudgementResult,
                    data: serde_json::to_value(&JudgementResponse {
                        address: NetAccount::from(&keypair.public),
                        judgement: Judgement::Reasonable,
                    }).unwrap()
                }
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::SendMessage {
                room_id: room_id.clone(),
                message: VerifierMessageBlank::Goodbye,
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::LeaveRoom {
                room_id: room_id,
            })
        ));
    });
}

#[test]
fn matrix_invalid_signature_response() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        // Setup database and manager.
        let db = Database2::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager2::new());
        let (_, matrix_child) = manager.child();

        let my_user_id = UserId::try_from("@registrar:matrix.org").unwrap();
        let matrix_transport = MatrixMocker::new(matrix_child, my_user_id);

        // Starts tasks.
        let handlers = test_run(
            Arc::clone(&manager),
            db,
            matrix_transport,
            DummyTransport::new(),
            DummyTransport::new(),
        )
        .await
        .unwrap();

        let matrix = handlers.matrix;
        let injector = handlers.reader.injector();

        let keypair = Keypair::generate();

        // Generate events.
        let msg = serde_json::to_string(&Message {
            event: EventType::NewJudgementRequest,
            data: serde_json::to_value(&JudgementRequest {
                address: NetAccount::from(&keypair.public),
                accounts: [(
                    AccountType::Matrix,
                    Some(Account::from("@alice:matrix.org")),
                )]
                .iter()
                .cloned()
                .collect(),
            })
            .unwrap(),
        })
        .unwrap();

        // Send new judgement request.
        injector.send_message(msg.clone()).await;
        pause().await;

        // Respond with invalid and valid signature.
        let signature =
            keypair.sign_simple(b"substrate", Challenge::gen_random().as_str().as_bytes());

        let room_id = RoomId::try_from("!17:matrix.org").unwrap();
        matrix.trigger_matrix_emitter(
            room_id.clone(),
            UserId::try_from("@registrar:matrix.org").unwrap(),
            MatrixEventMock {
                user_id: UserId::try_from("@alice:matrix.org").unwrap(),
                message: hex::encode(signature.to_bytes()),
            },
        );

        pause().await;

        // Verify events.
        let events = manager.events().await;
        println!("{:?}", events);

        // Skip startup events...

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: UserId::try_from("@alice:matrix.org").unwrap(),
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::SendMessage {
                room_id: room_id.clone(),
                message: VerifierMessageBlank::InitMessageWithContext,
            })
        ));

        assert!(events.contains(&
            Event::Matrix(MatrixEvent::SendMessage {
                room_id: room_id.clone(),
                message: VerifierMessageBlank::ResponseInvalid,
            })
        ));
    });
}
