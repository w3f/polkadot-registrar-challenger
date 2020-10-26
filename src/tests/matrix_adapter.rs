use super::mocks::*;
use super::{db_path, pause};
use crate::connector::{EventType, JudgementRequest, JudgementResponse, Message};
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
        assert_eq!(events.len(), 6);

        assert_eq!(
            events[0],
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::DisplayNamesRequest,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                }
            })
        );

        assert_eq!(
            events[1],
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::PendingJudgementsRequests,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                }
            })
        );

        assert_eq!(
            events[2],
            Event::Connector(ConnectorEvent::Reader { message: msg })
        );

        match &events[3] {
            Event::Connector(e) => match e {
                ConnectorEvent::Writer { message } => {
                    assert_eq!(message.event, EventType::Ack);
                }
                _ => panic!(),
            },
            _ => panic!(),
        }

        assert_eq!(
            events[4],
            Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: UserId::try_from("@alice:matrix.org").unwrap(),
            })
        );

        match &events[5] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::InitMessageWithContext(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }
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

        matrix.trigger_matrix_emitter(
            RoomId::try_from("!1234:matrix.org").unwrap(),
            UserId::try_from("@registrar:matrix.org").unwrap(),
            MatrixEventMock {
                user_id: UserId::try_from("@alice:matrix.org").unwrap(),
                message: hex::encode(signature.to_bytes()),
            },
        );

        pause().await;

        // Verify events.
        let events = manager.events().await;
        assert_eq!(events.len(), 10);

        // Skip startup events...

        assert_eq!(
            events[4],
            Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: UserId::try_from("@alice:matrix.org").unwrap(),
            })
        );

        match &events[5] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::InitMessageWithContext(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[6] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::ResponseValid(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[7] {
            Event::Connector(e) => match e {
                ConnectorEvent::Writer { message } => {
                    match message.event {
                        EventType::JudgementResult => {}
                        _ => panic!(),
                    }

                    assert_eq!(
                        serde_json::from_value::<JudgementResponse>(message.data.clone())
                            .unwrap()
                            .judgement,
                        Judgement::Reasonable
                    );
                }
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[8] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::Goodbye(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[9] {
            Event::Matrix(e) => match e {
                MatrixEvent::LeaveRoom { room_id: _ } => {}
                _ => panic!(),
            },
            _ => panic!(),
        }
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
        let invalid_signature =
            keypair.sign_simple(b"substrate", Challenge::gen_random().as_str().as_bytes());

        let valid_signature =
            keypair.sign_simple(b"substrate", Challenge::gen_fixed().as_str().as_bytes());

        matrix.trigger_matrix_emitter(
            RoomId::try_from("!1234:matrix.org").unwrap(),
            UserId::try_from("@registrar:matrix.org").unwrap(),
            MatrixEventMock {
                user_id: UserId::try_from("@alice:matrix.org").unwrap(),
                message: hex::encode(invalid_signature.to_bytes()),
            },
        );

        matrix.trigger_matrix_emitter(
            RoomId::try_from("!1234:matrix.org").unwrap(),
            UserId::try_from("@registrar:matrix.org").unwrap(),
            MatrixEventMock {
                user_id: UserId::try_from("@alice:matrix.org").unwrap(),
                message: hex::encode(valid_signature.to_bytes()),
            },
        );

        pause().await;

        // Verify events.
        let events = manager.events().await;
        assert_eq!(events.len(), 11);

        // Skip startup events...

        assert_eq!(
            events[4],
            Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: UserId::try_from("@alice:matrix.org").unwrap(),
            })
        );

        match &events[5] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::InitMessageWithContext(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[6] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::ResponseInvalid(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[7] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::ResponseValid(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[8] {
            Event::Connector(e) => match e {
                ConnectorEvent::Writer { message } => {
                    match message.event {
                        EventType::JudgementResult => {}
                        _ => panic!(),
                    }

                    assert_eq!(
                        serde_json::from_value::<JudgementResponse>(message.data.clone())
                            .unwrap()
                            .judgement,
                        Judgement::Reasonable
                    );
                }
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[9] {
            Event::Matrix(e) => match e {
                MatrixEvent::SendMessage {
                    room_id: _,
                    message,
                } => match message {
                    VerifierMessage::Goodbye(_) => {}
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }

        match &events[10] {
            Event::Matrix(e) => match e {
                MatrixEvent::LeaveRoom { room_id: _ } => {}
                _ => panic!(),
            },
            _ => panic!(),
        }
    });
}
