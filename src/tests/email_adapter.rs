use super::mocks::*;
use super::{db_path, pause};
use crate::adapters::email::{EmailId, ReceivedMessageContext};
use crate::connector::{EventType, JudgementRequest, JudgementResponse, Message, AckResponse};
use crate::primitives::{Account, AccountType, Challenge, Judgement, NetAccount};
use crate::verifier::VerifierMessage;
use crate::{test_run, Database2};
use schnorrkel::Keypair;
use std::sync::Arc;
use tokio::runtime::Runtime;

#[test]
fn email_init_message() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let db = Database2::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager2::new());
        let (writer, email_child) = manager.child();

        let email_transport = EmailMocker::new(email_child);

        let handlers = test_run(
            Arc::clone(&manager),
            db,
            DummyTransport::new(),
            DummyTransport::new(),
            email_transport,
        )
        .await
        .unwrap();

        let injector = handlers.reader.injector();

        // Generate events.
        let msg = serde_json::to_string(&Message {
            event: EventType::NewJudgementRequest,
            data: serde_json::to_value(&JudgementRequest {
                address: NetAccount::alice(),
                accounts: [(AccountType::Email, Some(Account::from("alice@email.com")))]
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

        assert!(events.contains(&Event::Connector(ConnectorEvent::Writer {
            message: Message {
                event: EventType::DisplayNamesRequest,
                data: serde_json::to_value(Option::<()>::None).unwrap(),
            }
        })));

        assert!(events.contains(&Event::Connector(ConnectorEvent::Writer {
            message: Message {
                event: EventType::PendingJudgementsRequests,
                data: serde_json::to_value(Option::<()>::None).unwrap(),
            }
        })));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Reader { message: msg })
        ));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::Ack,
                    data: serde_json::to_value(&AckResponse {
                        result: String::from("Message acknowledged"),
                    }).unwrap(),
                }
            })
        ));

        assert!(events.contains(&
            Event::Email(EmailEvent::SendMessage {
                account: Account::from("alice@email.com"),
                message: VerifierMessageBlank::InitMessageWithContext,
            })
        ));
    });
}

#[test]
fn email_valid_signature_response() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let db = Database2::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager2::new());
        let (writer, email_child) = manager.child();

        let email_transport = EmailMocker::new(email_child);

        let handlers = test_run(
            Arc::clone(&manager),
            db,
            DummyTransport::new(),
            DummyTransport::new(),
            email_transport,
        )
        .await
        .unwrap();

        let injector = handlers.reader.injector();
        let keypair = Keypair::generate();

        // Generate events.
        let msg = serde_json::to_string(&Message {
            event: EventType::NewJudgementRequest,
            data: serde_json::to_value(&JudgementRequest {
                address: NetAccount::from(&keypair.public),
                accounts: [(AccountType::Email, Some(Account::from("alice@email.com")))]
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

        writer
            .send_message(ReceivedMessageContext {
                id: EmailId::from(111u32),
                sender: Account::from("alice@email.com"),
                body: hex::encode(signature.to_bytes()),
            })
            .await;

        pause().await;

        // Verify events.
        let events = manager.events().await;
        println!("{:?}", events);

        // Skip startup events...

        assert!(events.contains(&Event::Connector(ConnectorEvent::Reader { message: msg })));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::Ack,
                    data: serde_json::to_value(&AckResponse {
                        result: String::from("Message acknowledged"),
                    }).unwrap()
                }
            })
        ));

        assert!(events.contains(&Event::Email(EmailEvent::SendMessage {
            account: Account::from("alice@email.com"),
            message: VerifierMessageBlank::InitMessageWithContext
        })));

        assert!(events.contains(&
            Event::Email(EmailEvent::RequestMessages {
                messages: vec![ReceivedMessageContext {
                    id: EmailId::from(111u32),
                    sender: Account::from("alice@email.com"),
                    body: hex::encode(signature.to_bytes()),
                }]
            })
        ));

        assert!(events.contains(&
            Event::Email(EmailEvent::SendMessage {
                account: Account::from("alice@email.com"),
                message: VerifierMessageBlank::ResponseValid,
            })
        ));
    });
}

#[test]
fn email_invalid_signature_response() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let db = Database2::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager2::new());
        let (writer, email_child) = manager.child();

        let email_transport = EmailMocker::new(email_child);

        let handlers = test_run(
            Arc::clone(&manager),
            db,
            DummyTransport::new(),
            DummyTransport::new(),
            email_transport,
        )
        .await
        .unwrap();

        let injector = handlers.reader.injector();
        let keypair = Keypair::generate();

        // Generate events.
        let msg = serde_json::to_string(&Message {
            event: EventType::NewJudgementRequest,
            data: serde_json::to_value(&JudgementRequest {
                address: NetAccount::from(&keypair.public),
                accounts: [(AccountType::Email, Some(Account::from("alice@email.com")))]
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

        writer
            .send_message(ReceivedMessageContext {
                id: EmailId::from(111u32),
                sender: Account::from("alice@email.com"),
                body: hex::encode(signature.to_bytes()),
            })
            .await;

        pause().await;

        // Verify events.
        let events = manager.events().await;

        // Skip startup events...

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Reader { message: msg })
        ));

        assert!(events.contains(&
            Event::Connector(ConnectorEvent::Writer {
                message: Message {
                    event: EventType::Ack,
                    data: serde_json::to_value(&AckResponse {
                        result: String::from("Message acknowledged"),
                    }).unwrap()
                }
            })
        ));

        assert!(events.contains(&
            Event::Email(EmailEvent::SendMessage {
                account: Account::from("alice@email.com"),
                message: VerifierMessageBlank::InitMessageWithContext,
            })
        ));

        assert!(events.contains(&
            Event::Email(EmailEvent::RequestMessages {
                messages: vec![ReceivedMessageContext {
                    id: EmailId::from(111u32),
                    sender: Account::from("alice@email.com"),
                    body: hex::encode(signature.to_bytes()),
                },]
            })
        ));

        assert!(events.contains(&
            Event::Email(EmailEvent::SendMessage {
                account: Account::from("alice@email.com"),
                message: VerifierMessageBlank::ResponseInvalid,
            })
        ));
    });
}
