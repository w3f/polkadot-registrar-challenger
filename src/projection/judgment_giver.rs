use super::Projection;
use crate::event::Event;

pub struct JudgmentGiver {}

#[async_trait]
impl Projection for JudgmentGiver {
    type Id = ();
    type Event = Event;
    type Error = anyhow::Error;

    async fn project(&mut self, event: Self::Event) -> std::result::Result<(), Self::Error> {
        unimplemented!()
    }
}
