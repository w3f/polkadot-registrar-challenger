use super::Projection;
use crate::event::Event;

pub struct JudgmentGiver {}

#[async_trait]
impl Projection for JudgmentGiver {
    type Id = ();
    type Event = Event;
    type Error = anyhow::Error;

    fn latest_revision(&self) -> u64 {
        unimplemented!()
    }

    fn update_revision(&mut self, revision: u64) {
        unimplemented!()
    }

    async fn project(&mut self, event: Self::Event) -> std::result::Result<(), Self::Error> {
        unimplemented!()
    }
}
