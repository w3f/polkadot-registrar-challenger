use super::{db_path, pause};
use super::mocks::*;
use crate::connector::{ConnectorWriterTransport, EventType, JudgementRequest, Message, AckResponse};
use crate::primitives::{Account, AccountType, NetAccount};
use crate::{test_run, Database2};
use crate::verifier::VerifierMessage;
use matrix_sdk::identifiers::{UserId, RoomId};
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::runtime::Runtime;

#[test]
fn verify_matrix() {
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

        // Verify events.
        let matrix = handlers.matrix;
        let mut writer = handlers.writer;
        let injector = handlers.reader.injector();

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
        }).unwrap();

        // Send new judgement request.
        injector.send_message(msg.clone()).await;
        pause().await;

        let events = manager.events().await;
        assert_eq!(events.len(), 6);

        assert_eq!(events[0],
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::DisplayNamesRequest,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                }
            })
        );

        assert_eq!(events[1],
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::PendingJudgementsRequests,
                    data: serde_json::to_value(Option::<()>::None).unwrap(),
                }
            })
        );

        assert_eq!(events[2],
            Event::Connector(ConnectorEvent::Reader {
                message: msg,
            })
        );

        match &events[3] {
            Event::Connector(e) => {
                match e {
                    ConnectorEvent::Writer {
                        message
                    } => {
                        assert_eq!(message.event, EventType::Ack);
                    }
                    _ => panic!()
                }
            }
            _ => panic!()
        }

        assert_eq!(events[4],
            Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: UserId::try_from("@alice:matrix.org").unwrap(),
            })
        );

        match &events[5] {
            Event::Matrix(e) => {
                match e {
                    MatrixEvent::SendMessage { room_id, message} => {
                        match message {
                            VerifierMessage::InitMessageWithContext(_) => {},
                            _ => panic!(),
                        }
                    }
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    });
}
