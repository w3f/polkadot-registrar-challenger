use super::mocks::*;
use super::{db_path, pause};
use crate::adapters::twitter::{ReceivedMessageContext, TwitterId};
use crate::connector::{AckResponse, EventType, JudgementRequest, Message};
use crate::primitives::{unix_time, Account, AccountType, NetAccount};
use crate::{test_run, Database};
use std::sync::Arc;
use tokio::runtime::Runtime;

#[test]
fn twitter_init_message() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let db = Database::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager::new());
        let (writer, twitter_child) = manager.child();

        let my_screen_name = Account::from("@registrar");
        let index_book = vec![
            (Account::from("@registrar"), TwitterId::from(111u64)),
            (Account::from("@alice"), TwitterId::from(222u64)),
        ];

        let twitter_transport =
            TwitterMocker::new(twitter_child, my_screen_name.clone(), index_book);

        let handlers = test_run(
            Arc::clone(&manager),
            db,
            Default::default(),
            DummyTransport::new(),
            twitter_transport,
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
                accounts: [(AccountType::Twitter, Some(Account::from("@alice")))]
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

        let sender_watermark = unix_time();
        writer
            .send_message(ReceivedMessageContext {
                sender: TwitterId::from(222u64),
                message: String::from("Hello, World!"),
                created: sender_watermark,
            })
            .await;

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

        assert!(
            events.contains(&Event::Twitter(TwitterEvent::LookupTwitterId {
                twitter_ids: None,
                accounts: Some(vec![my_screen_name.clone(),]),
                lookups: vec![(my_screen_name, TwitterId::from(111u64)),],
            }))
        );

        assert!(
            events.contains(&Event::Twitter(TwitterEvent::RequestMessages {
                exclude: TwitterId::from(111u64),
                watermark: 0,
                messages: vec![ReceivedMessageContext {
                    sender: TwitterId::from(222u64),
                    message: String::from("Hello, World!"),
                    created: sender_watermark,
                }]
            }))
        );

        assert!(events.contains(&Event::Connector(ConnectorEvent::Reader { message: msg })));

        assert!(events.contains(&Event::Connector(ConnectorEvent::Writer {
            message: Message {
                event: EventType::Ack,
                data: serde_json::to_value(&AckResponse {
                    result: String::from("Message acknowledged"),
                })
                .unwrap()
            }
        })));

        assert_eq!(
            events[6],
            Event::Twitter(TwitterEvent::LookupTwitterId {
                twitter_ids: Some(vec![TwitterId::from(222u64),]),
                accounts: None,
                lookups: vec![(Account::from("@alice"), TwitterId::from(222u64)),],
            })
        );

        match &events[7] {
            Event::Twitter(e) => match e {
                TwitterEvent::SendMessage { id, message } => {
                    assert_eq!(id, &TwitterId::from(222u64));

                    match message {
                        VerifierMessageBlank::InitMessage => {}
                        _ => panic!(),
                    }
                }
                _ => panic!(),
            },
            _ => panic!(),
        }
    });
}
