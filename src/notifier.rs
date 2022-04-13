use crate::actors::api::{LookupServer, NotifyAccountState};
use crate::database::Database;
use crate::primitives::{IdentityContext, JudgementState, Timestamp};
use crate::Result;
use actix::prelude::*;
use std::collections::HashMap;
use tokio::time::{sleep, Duration};

// TODO: Should be a single function.
#[derive(Clone)]
pub struct SessionNotifier {
    db: Database,
    server: Addr<LookupServer>,
}

impl SessionNotifier {
    pub fn new(db: Database, server: Addr<LookupServer>) -> Self {
        SessionNotifier { db, server }
    }
    pub async fn run_blocking(mut self) {
        async fn local(
            db: &mut Database,
            server: &Addr<LookupServer>,
            event_counter: &mut u64,
        ) -> Result<()> {
            let (events, new_counter) = db.fetch_events(*event_counter).await?;
            let mut cache: HashMap<IdentityContext, JudgementState> = HashMap::new();

            for event in events {
                let state = match cache.get(event.context()) {
                    Some(state) => state.clone(),
                    None => {
                        let state = db
                            .fetch_judgement_state(event.context())
                            .await?
                            .ok_or_else(|| {
                                anyhow!(
                                    "No identity state found for context: {:?}",
                                    event.context()
                                )
                            })?;

                        cache.insert(event.context().clone(), state.clone());

                        state
                    }
                };

                // TODO: Pass multiple events of the same identity as one.
                server.do_send(NotifyAccountState {
                    state: state.into(),
                    notifications: vec![event],
                });
            }

            *event_counter = new_counter;

            Ok(())
        }

        let mut event_counter = Timestamp::now().raw();
        loop {
            if let Err(err) = local(&mut self.db, &self.server, &mut event_counter).await {
                error!("Error in session notifier event loop: {:?}", err);
            }

            // Fetch events based on intervals until ["Change
            // Streams"](https://docs.mongodb.com/manual/changeStreams/) are
            // implemented in the Rust MongoDb driver.
            sleep(Duration::from_secs(1)).await;
        }
    }
}
