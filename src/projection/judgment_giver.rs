use super::Projection;
use crate::event::{Event, EventType, OnChainRemark};
use crate::manager::{NetworkAddress, OnChainChallenge};
use std::collections::HashMap;

pub struct JudgmentGiver {
    remarks: HashMap<NetworkAddress, OnChainRemark>,
    pending: HashMap<NetworkAddress, OnChainChallenge>,
}

#[async_trait]
impl Projection for JudgmentGiver {
    type Id = ();
    type Event = Event;
    type Error = anyhow::Error;

    async fn project(&mut self, event: Self::Event) -> std::result::Result<(), Self::Error> {
        match event.body {
            EventType::IdentityFullyVerified(identity) => {
                self.pending
                    .insert(identity.net_address, identity.on_chain_challenge);
            }
            EventType::RemarkFound(remark) => {
                self.remarks.insert(remark.net_address, remark.remark);
            }
            _ => {}
        }

        Ok(())
    }
}
