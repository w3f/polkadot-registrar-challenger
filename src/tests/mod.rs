use std::process::{Command, Child};
use rand::{thread_rng, Rng};

mod api_account_status;

struct InMemEventStore {
    handle: Child,
}

impl InMemEventStore {
    fn run() -> Self {
        let port: usize = thread_rng().gen_range(1_024, 65_535);
        let handle = Command::new(format!("eventstored --mem-db --disable-admin-ui --http-port {}", port)).spawn().unwrap();

        InMemEventStore {
            handle: handle,
        }
    }
}

impl Drop for InMemEventStore {
    fn drop(&mut self) {
        self.handle.kill().unwrap();
    }
}
