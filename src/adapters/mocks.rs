use super::email;
use super::twitter::{self, TwitterError, TwitterId};
use super::{EmailTransport, MatrixTransport, TwitterTransport};
use crate::comms::CommsVerifier;
use crate::manager::OnChainIdentity;
use crate::primitives::{unix_time, AccountType, NetAccount, Result};
use crate::{Account, Database2};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::stream::TryStreamExt;
use futures_channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use matrix_sdk::api::r0::room::create_room::{Request, Response};
use matrix_sdk::identifiers::{RoomId, UserId};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::result::Result as StdResult;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Matrix(MatrixEvent),
    Email(EmailEvent),
    Twitter(TwitterEvent),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MatrixEvent {
    SendMessage { room_id: RoomId, message: String },
    CreateRoom { to_invite: UserId },
    LeaveRoom { room_id: RoomId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EmailEvent {
    RequestMessages {
        messages: Vec<email::ReceivedMessageContext>,
    },
    SendMessage {
        account: Account,
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TwitterEvent {
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

struct EventManager2 {
    events: Arc<RwLock<Vec<Event>>>,
}

impl EventManager2 {
    fn new() -> Self {
        EventManager2 {
            events: Arc::new(RwLock::new(vec![])),
        }
    }
    fn child<T>(&self) -> (EventChildSender<T>, EventChild<T>) {
        let sender = EventChildSender {
            messages: Arc::new(RwLock::new(vec![])),
        };

        let child = EventChild {
            events: Arc::clone(&self.events),
            messages: Arc::clone(&sender.messages),
        };

        (sender, child)
    }
    async fn events(&self) -> Vec<Event> {
        self.events.read().await.clone()
    }
}

struct EventChildSender<T> {
    messages: Arc<RwLock<Vec<T>>>,
}

impl<T> EventChildSender<T> {
    async fn send_message(&self, messages: T) {
        self.messages.write().await.push(messages);
    }
}

struct EventChild<T> {
    events: Arc<RwLock<Vec<Event>>>,
    messages: Arc<RwLock<Vec<T>>>,
}

impl<T: Clone> EventChild<T> {
    async fn messages(&self) -> Vec<T> {
        self.messages.read().await.clone()
    }
    async fn push_event(&self, event: Event) {
        self.events.write().await.push(event);
    }
}

pub struct MatrixMocker {
    child: EventChild<()>,
    user_id: UserId,
}

impl MatrixMocker {
    fn new(child: EventChild<()>, user_id: UserId) -> Self {
        MatrixMocker {
            child: child,
            user_id: user_id,
        }
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
    async fn run_emitter(&mut self, db: Database2, comms: CommsVerifier) {
        unimplemented!()
    }
}

pub struct EmailMocker {
    child: EventChild<email::ReceivedMessageContext>,
}

impl EmailMocker {
    fn new(child: EventChild<email::ReceivedMessageContext>) -> Self {
        EmailMocker { child: child }
    }
}

#[async_trait]
impl EmailTransport for EmailMocker {
    async fn request_messages(&self) -> Result<Vec<email::ReceivedMessageContext>> {
        let mut messages = self.child.messages().await;

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
    fn new(
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
        let mut messages = self.child.messages().await;

        let mut new_watermark = 0;
        let messages = messages
            .into_iter()
            .filter(|message| {
                if message.created > new_watermark {
                    new_watermark = message.created;
                }

                message.created > watermark
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
    use std::mem::drop;
    use tokio::runtime::Runtime;

    #[test]
    fn matrix_mocker() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Setup manager.
            let mut manager = EventManager2::new();
            let (sender, matrix_child) = manager.child();

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
                .send_message(&room_id, String::from("Hello"))
                .await
                .unwrap();
            mocker
                .send_message(&room_id, String::from("World"))
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
                    message: String::from("Hello")
                })
            );
            assert_eq!(
                events[2],
                Event::Matrix(MatrixEvent::SendMessage {
                    room_id: room_id.clone(),
                    message: String::from("World")
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
            let mut manager = EventManager2::new();
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
}
