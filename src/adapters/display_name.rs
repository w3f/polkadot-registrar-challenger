use crate::Database2;
use crate::comms::CommsVerifier;
use crate::primitives::Result;

pub struct StringMatcher {
    db: Database2,
    comms: CommsVerifier,
}

impl StringMatcher {
    pub fn new(db: Database2, comms: CommsVerifier) -> Self {
        StringMatcher {
            db: db,
            comms: comms,
        }
    }
    pub async fn start(self) {
        loop {
            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
                err
            });
        }
    }
    pub async fn local(&self) -> Result<()> {

        Ok(())
    }
}
