use crate::adapters::email;
use crate::adapters::twitter::{self, TwitterError, TwitterId};
use crate::adapters::{EmailTransport, EventExtract, MatrixTransport, TwitterTransport};
use crate::comms::CommsVerifier;
use crate::connector::{
    ConnectorInitTransports, ConnectorReaderTransport, ConnectorWriterTransport, EventType, Message,
};
use crate::primitives::{unix_time, Result};
use crate::{Account, Database2};
use matrix_sdk::api::r0::room::create_room::{Request, Response};
use matrix_sdk::identifiers::{RoomId, UserId};
use std::convert::TryFrom;
use std::result::Result as StdResult;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Matrix(MatrixEvent),
    Email(EmailEvent),
    Twitter(TwitterEvent),
    Connector(ConnectorEvent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatrixEvent {
    SendMessage { room_id: RoomId, message: String },
    CreateRoom { to_invite: UserId },
    LeaveRoom { room_id: RoomId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmailEvent {
    RequestMessages {
        messages: Vec<email::ReceivedMessageContext>,
    },
    SendMessage {
        account: Account,
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TwitterEvent {
    RequestMessages {
        exclude: TwitterId,
        watermark: u64,
        messages: Vec<twitter::ReceivedMessageContext>,
    },
    LookupTwitterId {
        twitter_ids: Option<Vec<TwitterId>>,
        accounts: Option<Vec<Account>>,
        lookups: Vec<(Account, TwitterId)>,
    },
    SendMessage {
        id: TwitterId,
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectorEvent {
    Writer { message: Message },
    Reader { message: Option<String> },
}

pub struct EventManager2 {
    events: Arc<RwLock<Vec<Event>>>,
}

impl EventManager2 {
    pub fn new() -> Self {
        EventManager2 {
            events: Arc::new(RwLock::new(vec![])),
        }
    }
    pub fn child<T>(&self) -> (EventChildSender<T>, EventChild<T>) {
        let sender = EventChildSender {
            messages: Arc::new(RwLock::new(vec![])),
        };

        let child = EventChild {
            events: Arc::clone(&self.events),
            messages: Arc::clone(&sender.messages),
        };

        (sender, child)
    }
    pub async fn events(&self) -> Vec<Event> {
        self.events.read().await.clone()
    }
}

#[derive(Clone)]
pub struct EventChildSender<T> {
    messages: Arc<RwLock<Vec<T>>>,
}

impl<T> EventChildSender<T> {
    pub async fn send_message(&self, message: T) {
        self.messages.write().await.push(message);
    }
}

#[derive(Clone)]
pub struct EventChild<T> {
    events: Arc<RwLock<Vec<Event>>>,
    messages: Arc<RwLock<Vec<T>>>,
}

impl<T: Clone> EventChild<T> {
    pub async fn messages(&self) -> Vec<T> {
        self.messages.read().await.clone()
    }
    pub async fn fifo_message(&self) -> Option<T> {
        let mut messages = self.messages.write().await;
        if !messages.is_empty() {
            Some(messages.remove(0))
        } else {
            None
        }
    }
    pub async fn push_event(&self, event: Event) {
        self.events.write().await.push(event);
    }
}

pub struct ConnectorMocker {}

#[async_trait]
impl ConnectorInitTransports<ConnectorWriterMocker, ConnectorReaderMocker> for ConnectorMocker {
    type Endpoint = Arc<EventManager2>;

    async fn init(
        endpoint: Self::Endpoint,
    ) -> Result<(ConnectorWriterMocker, ConnectorReaderMocker)> {
        let (sender, child) = endpoint.child();

        Ok((
            ConnectorWriterMocker {
                child: endpoint.child().1,
            },
            ConnectorReaderMocker {
                sender: sender,
                child: child,
            },
        ))
    }
}

pub struct ConnectorWriterMocker {
    child: EventChild<()>,
}

#[async_trait]
impl ConnectorWriterTransport for ConnectorWriterMocker {
    async fn write(&mut self, message: &Message) -> Result<()> {
        self.child
            .push_event(Event::Connector(ConnectorEvent::Writer {
                message: message.clone(),
            }))
            .await;
        Ok(())
    }
}

pub struct ConnectorReaderMocker {
    sender: EventChildSender<String>,
    child: EventChild<String>,
}

impl ConnectorReaderMocker {
    fn sender(&self) -> EventChildSender<String> {
        self.sender.clone()
    }
}

#[async_trait]
impl ConnectorReaderTransport for ConnectorReaderMocker {
    async fn read(&mut self) -> Result<Option<String>> {
        let message = self.child.fifo_message().await;
        self.child
            .push_event(Event::Connector(ConnectorEvent::Reader {
                message: message.clone(),
            }))
            .await;
        Ok(message)
    }
}

pub struct MatrixMocker {
    child: EventChild<()>,
    user_id: UserId,
}

impl MatrixMocker {
    pub fn new(child: EventChild<()>, user_id: UserId) -> Self {
        MatrixMocker {
            child: child,
            user_id: user_id,
        }
    }
}

#[derive(Clone)]
pub struct DummyTransport {
    screen_name: Account,
}

impl DummyTransport {
    pub fn new() -> Self {
        DummyTransport {
            screen_name: Account::from(""),
        }
    }
}

#[async_trait]
impl MatrixTransport for DummyTransport {
    async fn send_message(&self, _room_id: &RoomId, _message: String) -> Result<()> {
        unimplemented!()
    }
    async fn create_room<'a>(&'a self, _request: Request<'a>) -> Result<Response> {
        unimplemented!()
    }
    async fn leave_room(&self, _room_id: &RoomId) -> Result<()> {
        unimplemented!()
    }
    async fn user_id(&self) -> Result<UserId> {
        unimplemented!()
    }
    async fn run_emitter(&mut self, _db: Database2, _comms: CommsVerifier) {}
}

#[async_trait]
impl EmailTransport for DummyTransport {
    async fn request_messages(&self) -> Result<Vec<email::ReceivedMessageContext>> {
        unimplemented!()
    }
    async fn send_message(&self, _account: &Account, _msg: String) -> Result<()> {
        unimplemented!()
    }
}

#[async_trait]
impl TwitterTransport for DummyTransport {
    async fn request_messages(
        &self,
        _exclude: &TwitterId,
        _watermark: u64,
    ) -> Result<(Vec<twitter::ReceivedMessageContext>, u64)> {
        Ok((vec![], 0))
    }
    async fn lookup_twitter_id(
        &self,
        _twitter_ids: Option<&[&TwitterId]>,
        _accounts: Option<&[&Account]>,
    ) -> Result<Vec<(Account, TwitterId)>> {
        Ok(vec![(Account::from(""), TwitterId::from(0))])
    }
    async fn send_message(&self, _id: &TwitterId, _message: String) -> StdResult<(), TwitterError> {
        unimplemented!()
    }
    fn my_screen_name(&self) -> &Account {
        &self.screen_name
    }
}

#[async_trait]
impl MatrixTransport for MatrixMocker {
    async fn send_message(&self, room_id: &RoomId, message: String) -> Result<()> {
        self.child
            .push_event(Event::Matrix(MatrixEvent::SendMessage {
                room_id: room_id.clone(),
                message: message,
            }))
            .await;

        Ok(())
    }
    async fn create_room<'a>(&'a self, request: Request<'a>) -> Result<Response> {
        self.child
            .push_event(Event::Matrix(MatrixEvent::CreateRoom {
                to_invite: request.invite[0].clone(),
            }))
            .await;

        Ok(Response::new(
            RoomId::try_from(format!("!{}:matrix.org", unix_time()).as_str()).unwrap(),
        ))
    }
    async fn leave_room(&self, room_id: &RoomId) -> Result<()> {
        self.child
            .push_event(Event::Matrix(MatrixEvent::LeaveRoom {
                room_id: room_id.clone(),
            }))
            .await;

        Ok(())
    }
    async fn user_id(&self) -> Result<UserId> {
        Ok(self.user_id.clone())
    }
    async fn run_emitter(&mut self, _db: Database2, _comms: CommsVerifier) {
        unimplemented!()
    }
}

pub struct MatrixEventMock {
    user_id: UserId,
    message: Result<String>,
}

impl EventExtract for MatrixEventMock {
    fn sender(&self) -> &UserId {
        &self.user_id
    }
    fn message(&self) -> Result<String> {
        // Work around ownership violations.
        if let Ok(message) = &self.message {
            Ok(message.clone())
        } else {
            Err(failure::err_msg(""))
        }
    }
}

pub struct EmailMocker {
    child: EventChild<email::ReceivedMessageContext>,
}

impl EmailMocker {
    pub fn new(child: EventChild<email::ReceivedMessageContext>) -> Self {
        EmailMocker { child: child }
    }
}

#[async_trait]
impl EmailTransport for EmailMocker {
    async fn request_messages(&self) -> Result<Vec<email::ReceivedMessageContext>> {
        let messages = self.child.messages().await;

        self.child
            .push_event(Event::Email(EmailEvent::RequestMessages {
                messages: messages.clone(),
            }))
            .await;

        Ok(messages)
    }
    async fn send_message(&self, account: &Account, msg: String) -> Result<()> {
        self.child
            .push_event(Event::Email(EmailEvent::SendMessage {
                account: account.clone(),
                message: msg,
            }))
            .await;

        Ok(())
    }
}

pub struct TwitterMocker {
    child: EventChild<twitter::ReceivedMessageContext>,
    index_book: Vec<(Account, TwitterId)>,
    screen_name: Account,
}

impl TwitterMocker {
    pub fn new(
        child: EventChild<twitter::ReceivedMessageContext>,
        screen_name: Account,
        index_book: Vec<(Account, TwitterId)>,
    ) -> Self {
        TwitterMocker {
            child: child,
            index_book: index_book,
            screen_name: screen_name,
        }
    }
}

#[async_trait]
impl TwitterTransport for TwitterMocker {
    async fn request_messages(
        &self,
        exclude: &TwitterId,
        watermark: u64,
    ) -> Result<(Vec<twitter::ReceivedMessageContext>, u64)> {
        let messages = self.child.messages().await;

        let mut new_watermark = 0;
        let messages = messages
            .into_iter()
            .filter(|message| {
                if message.created > new_watermark {
                    new_watermark = message.created;
                }

                message.created > watermark && &message.sender != exclude
            })
            .collect::<Vec<twitter::ReceivedMessageContext>>();

        self.child
            .push_event(Event::Twitter(TwitterEvent::RequestMessages {
                exclude: exclude.clone(),
                watermark: watermark,
                messages: messages.clone(),
            }))
            .await;

        Ok((messages, new_watermark))
    }
    async fn lookup_twitter_id(
        &self,
        twitter_ids: Option<&[&TwitterId]>,
        accounts: Option<&[&Account]>,
    ) -> Result<Vec<(Account, TwitterId)>> {
        let mut lookups = vec![];

        if let Some(twitter_ids) = twitter_ids {
            for twitter_id in twitter_ids {
                let pair = self
                    .index_book
                    .iter()
                    .find(|(_, id)| id == *twitter_id)
                    .unwrap();

                lookups.push(pair.clone());
            }
        }

        if let Some(accounts) = accounts {
            for account in accounts {
                let pair = self
                    .index_book
                    .iter()
                    .find(|(acc, _)| acc == *account)
                    .unwrap();

                lookups.push(pair.clone());
            }
        }

        self.child
            .push_event(Event::Twitter(TwitterEvent::LookupTwitterId {
                twitter_ids: twitter_ids.map(|ids| ids.iter().map(|&id| id.clone()).collect()),
                accounts: accounts
                    .map(|accounts| accounts.iter().map(|&account| account.clone()).collect()),
                lookups: lookups.clone(),
            }))
            .await;

        Ok(lookups)
    }
    async fn send_message(&self, id: &TwitterId, message: String) -> StdResult<(), TwitterError> {
        self.child
            .push_event(Event::Twitter(TwitterEvent::SendMessage {
                id: id.clone(),
                message: message,
            }))
            .await;

        Ok(())
    }
    fn my_screen_name(&self) -> &Account {
        &self.screen_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::EmailId;
    use tokio::runtime::Runtime;

    #[test]
    fn matrix_mocker() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Setup manager.
            let manager = EventManager2::new();
            let (_sender, matrix_child) = manager.child();

            // Prepare variables.
            let my_user_id = UserId::try_from("@registrar:matrix.org").unwrap();
            let room_id = RoomId::try_from("!1234:matrix.org").unwrap();

            let mut request = Request::new();
            let to_invite = UserId::try_from("@alice:matrix.org").unwrap();
            let to_invite_list = [to_invite.clone()];
            request.invite = &to_invite_list;

            // Init mocker and create events.
            let mocker = MatrixMocker::new(matrix_child, my_user_id.clone());

            mocker.create_room(request).await.unwrap();

            mocker
                .send_message(&room_id, String::from("First message out"))
                .await
                .unwrap();
            mocker
                .send_message(&room_id, String::from("Second message out"))
                .await
                .unwrap();

            mocker.leave_room(&room_id).await.unwrap();

            // Verify events.
            let events = manager.events().await;
            assert_eq!(events.len(), 4);

            assert_eq!(
                events[0],
                Event::Matrix(MatrixEvent::CreateRoom {
                    to_invite: to_invite.clone()
                })
            );
            assert_eq!(
                events[1],
                Event::Matrix(MatrixEvent::SendMessage {
                    room_id: room_id.clone(),
                    message: String::from("First message out")
                })
            );
            assert_eq!(
                events[2],
                Event::Matrix(MatrixEvent::SendMessage {
                    room_id: room_id.clone(),
                    message: String::from("Second message out")
                })
            );
            assert_eq!(
                events[3],
                Event::Matrix(MatrixEvent::LeaveRoom {
                    room_id: room_id.clone()
                })
            );
        });
    }

    #[test]
    fn email_mocker() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Setup manager
            let manager = EventManager2::new();
            let (sender, email_child) = manager.child();

            // Prepare variables
            let alice = Account::from("alice@example.com");
            let bob = Account::from("bob@example.com");

            let alice_message = email::ReceivedMessageContext {
                id: EmailId::from(11u64),
                sender: alice.clone(),
                body: String::from("from alice one"),
            };

            let bob_message = email::ReceivedMessageContext {
                id: EmailId::from(33u64),
                sender: bob.clone(),
                body: String::from("from bob one"),
            };

            // Init mocker and create events.
            let mocker = EmailMocker::new(email_child);

            let res = mocker.request_messages().await.unwrap();
            assert_eq!(res, vec![]);

            mocker
                .send_message(&alice, String::from("alice one"))
                .await
                .unwrap();
            mocker
                .send_message(&alice, String::from("alice two"))
                .await
                .unwrap();
            mocker
                .send_message(&bob, String::from("bob one"))
                .await
                .unwrap();

            // Fill buffer.
            sender.send_message(alice_message.clone()).await;
            sender.send_message(bob_message.clone()).await;

            let res = mocker.request_messages().await.unwrap();
            assert_eq!(res.len(), 2);
            assert!(res.contains(&alice_message));
            assert!(res.contains(&bob_message));

            // Verify events.
            let events = manager.events().await;
            assert_eq!(events.len(), 5);

            assert_eq!(
                events[0],
                Event::Email(EmailEvent::RequestMessages { messages: vec![] })
            );
            assert_eq!(
                events[1],
                Event::Email(EmailEvent::SendMessage {
                    account: alice.clone(),
                    message: String::from("alice one"),
                })
            );
            assert_eq!(
                events[2],
                Event::Email(EmailEvent::SendMessage {
                    account: alice.clone(),
                    message: String::from("alice two"),
                })
            );
            assert_eq!(
                events[3],
                Event::Email(EmailEvent::SendMessage {
                    account: bob.clone(),
                    message: String::from("bob one"),
                })
            );
            assert_eq!(
                events[4],
                Event::Email(EmailEvent::RequestMessages {
                    messages: vec![alice_message.clone(), bob_message.clone(),]
                })
            );
        });
    }

    #[test]
    fn twitter_mocker() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Setup manager.
            let manager = EventManager2::new();
            let (sender, twitter_child) = manager.child();

            // Prepare variables.
            let screen_name = Account::from("@registrar");
            let my_id = TwitterId::from(111u64);

            let alice = Account::from("@alice");
            let alice_id = TwitterId::from(222u64);

            let bob = Account::from("@bob");
            let bob_id = TwitterId::from(333u64);

            let inbox_book = vec![
                (screen_name.clone(), my_id.clone()),
                (alice.clone(), alice_id.clone()),
                (bob.clone(), bob_id.clone()),
            ];

            let alice_message1 = twitter::ReceivedMessageContext {
                sender: alice_id.clone(),
                message: String::from("from alice one"),
                created: 22,
            };

            let alice_message2 = twitter::ReceivedMessageContext {
                sender: alice_id.clone(),
                message: String::from("from alice one"),
                created: 33,
            };

            let my_message = twitter::ReceivedMessageContext {
                sender: my_id.clone(),
                message: String::from("from me one"),
                created: 44,
            };

            let bob_message = twitter::ReceivedMessageContext {
                sender: bob_id.clone(),
                message: String::from("from bob one"),
                created: 55,
            };

            // Init mocker and create events.
            let mocker = TwitterMocker::new(twitter_child, screen_name.clone(), inbox_book);

            let lookups = mocker
                .lookup_twitter_id(None, Some(&[mocker.my_screen_name()]))
                .await
                .unwrap();
            assert_eq!(lookups.len(), 1);
            assert!(lookups.contains(&(screen_name.clone(), my_id.clone())));

            let res = mocker.request_messages(&my_id, 0).await.unwrap();
            assert_eq!(res.0.len(), 0);
            assert_eq!(res.1, 0);

            let lookups = mocker
                .lookup_twitter_id(Some(&[&my_id, &alice_id]), Some(&[&bob]))
                .await
                .unwrap();

            assert_eq!(lookups.len(), 3);
            assert!(lookups.contains(&(screen_name.clone(), my_id.clone())));
            assert!(lookups.contains(&(alice.clone(), alice_id.clone())));
            assert!(lookups.contains(&(bob.clone(), bob_id.clone())));

            mocker
                .send_message(&alice_id, String::from("alice one"))
                .await
                .unwrap();
            mocker
                .send_message(&bob_id, String::from("bob one"))
                .await
                .unwrap();

            // Fill buffer.
            sender.send_message(alice_message1.clone()).await;
            sender.send_message(alice_message2.clone()).await;
            sender.send_message(my_message.clone()).await;
            sender.send_message(bob_message.clone()).await;

            let (res, watermark) = mocker.request_messages(&my_id, 30).await.unwrap();
            assert_eq!(res.len(), 2);
            assert_eq!(watermark, 55);
            assert!(res.contains(&alice_message2));
            assert!(res.contains(&bob_message));

            // Verify events;
            let events = manager.events().await;
            assert_eq!(events.len(), 6);

            assert_eq!(
                events[0],
                Event::Twitter(TwitterEvent::LookupTwitterId {
                    twitter_ids: None,
                    accounts: Some(vec![screen_name.clone(),]),
                    lookups: vec![(screen_name.clone(), my_id.clone())],
                })
            );
            assert_eq!(
                events[1],
                Event::Twitter(TwitterEvent::RequestMessages {
                    exclude: my_id.clone(),
                    watermark: 0,
                    messages: vec![],
                })
            );
            assert_eq!(
                events[2],
                Event::Twitter(TwitterEvent::LookupTwitterId {
                    twitter_ids: Some(vec![my_id.clone(), alice_id.clone(),]),
                    accounts: Some(vec![bob.clone(),]),
                    lookups: vec![
                        (screen_name.clone(), my_id.clone()),
                        (alice.clone(), alice_id.clone()),
                        (bob.clone(), bob_id.clone()),
                    ],
                })
            );
            assert_eq!(
                events[3],
                Event::Twitter(TwitterEvent::SendMessage {
                    id: alice_id.clone(),
                    message: String::from("alice one"),
                })
            );
            assert_eq!(
                events[4],
                Event::Twitter(TwitterEvent::SendMessage {
                    id: bob_id.clone(),
                    message: String::from("bob one"),
                })
            );
            assert_eq!(
                events[5],
                Event::Twitter(TwitterEvent::RequestMessages {
                    exclude: my_id.clone(),
                    watermark: 30,
                    messages: vec![alice_message2, bob_message],
                })
            );
        });
    }

    #[test]
    fn connector_mocker() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Setup manager.
            let manager = Arc::new(EventManager2::new());

            // Init mocker and create events.
            let (mut writer, mut reader) =
                ConnectorMocker::init(Arc::clone(&manager)).await.unwrap();
            let sender = reader.sender();

            let res = reader.read().await.unwrap();
            assert!(res.is_none());

            writer
                .write(&Message {
                    event: EventType::Ack,
                    data: serde_json::to_value("First message out").unwrap(),
                })
                .await
                .unwrap();

            writer
                .write(&Message {
                    event: EventType::Error,
                    data: serde_json::to_value("Second message out").unwrap(),
                })
                .await
                .unwrap();

            sender.send_message(String::from("First message in")).await;
            sender.send_message(String::from("Second message in")).await;

            let res = reader.read().await.unwrap().unwrap();
            assert_eq!(res, String::from("First message in"));
            let res = reader.read().await.unwrap().unwrap();
            assert_eq!(res, String::from("Second message in"));

            let res = reader.read().await.unwrap();
            assert!(res.is_none());

            // Verify events/
            let events = manager.events().await;
            assert_eq!(events.len(), 6);

            assert_eq!(
                events[0],
                Event::Connector(ConnectorEvent::Reader { message: None })
            );
            assert_eq!(
                events[1],
                Event::Connector(ConnectorEvent::Writer {
                    message: Message {
                        event: EventType::Ack,
                        data: serde_json::to_value("First message out").unwrap(),
                    },
                })
            );
            assert_eq!(
                events[2],
                Event::Connector(ConnectorEvent::Writer {
                    message: Message {
                        event: EventType::Error,
                        data: serde_json::to_value("Second message out").unwrap(),
                    },
                })
            );
            assert_eq!(
                events[3],
                Event::Connector(ConnectorEvent::Reader {
                    message: Some(String::from("First message in")),
                })
            );
            assert_eq!(
                events[4],
                Event::Connector(ConnectorEvent::Reader {
                    message: Some(String::from("Second message in")),
                })
            );
            assert_eq!(
                events[5],
                Event::Connector(ConnectorEvent::Reader { message: None })
            );
        });
    }
}
