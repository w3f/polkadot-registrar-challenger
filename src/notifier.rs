use crate::actors::api::{LookupServer, NotifyAccountState};
use crate::database::Database;
use crate::primitives::{IdentityContext, JudgementState, Timestamp};
use actix::prelude::*;
use std::collections::HashMap;
use tokio::time::{interval, Duration};

// TODO: Should be a single function.
#[derive(Clone)]
pub struct SessionNotifier {
    db: Database,
    server: Addr<LookupServer>,
}

impl SessionNotifier {
    pub fn new(db: Database, server: Addr<LookupServer>) -> Self {
        SessionNotifier {
            db: db,
            server: server,
        }
    }
    pub async fn run_blocking(mut self) {
        let mut interval = interval(Duration::from_secs(1));

        //let mut event_counter = Timestamp::now().raw();
        let mut event_counter = Timestamp::now().raw();
        loop {
            interval.tick().await;

            // Fetch events based on intervals until ["Change
            // Streams"](https://docs.mongodb.com/manual/changeStreams/) are
            // implemented in the Rust MongoDb driver.
            match self.db.fetch_events(event_counter).await {
                Ok((events, new_counter)) => {
                    let mut cache: HashMap<IdentityContext, JudgementState> = HashMap::new();

                    for event in events {
                        let state = match cache.get(event.context()) {
                            Some(state) => state.clone(),
                            None => {
                                let state = self
                                    .db
                                    .fetch_judgement_state(event.context())
                                    .await
                                    // TODO: Handle unwrap
                                    .unwrap()
                                    .ok_or(anyhow!(
                                        "No identity state found for context: {:?}",
                                        event.context()
                                    ))
                                    .unwrap();

                                cache.insert(event.context().clone(), state.clone());

                                state
                            }
                        };

                        // TODO: Pass multiple events of the same identity as one.
                        self.server.do_send(NotifyAccountState {
                            state: state.into(),
                            notifications: vec![event],
                        });
                    }

                    event_counter = new_counter;
                }
                Err(err) => {
                    error!("Error fetching events from database: {:?}", err);
                }
            }
        }
    }
}
