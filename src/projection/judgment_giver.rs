use super::Projection;
use crate::event::{Event, EventType, RemarkFound};
use crate::manager::{NetworkAddress, OnChainChallenge};
use std::collections::HashMap;

pub struct JudgmentGiver {
    remarks: HashMap<NetworkAddress, RemarkFound>,
    pending: HashMap<NetworkAddress, OnChainChallenge>,
}

#[async_trait]
impl Projection for JudgmentGiver {
    type Id = ();
    type Event = Event;
    type Error = anyhow::Error;

    // TODO: Cleanup after judgement confirmation.
    async fn project(&mut self, event: Self::Event) -> std::result::Result<(), Self::Error> {
        match event.body {
            EventType::IdentityFullyVerified(identity) => {
                // It's very unlikely that the remark is set on-chain before the
                // identity is verified. However, the challenge can be fetched
                // via the API so this case must be handled.
                if let Some(remark) = self.remarks.get(&identity.net_address) {
                    if identity.on_chain_challenge.matches_remark(&remark) {
                        // TODO: Send judgement to watcher.
                        info!(
                            "Valid remark found for {}, submitting valid judgement",
                            identity.net_address.address_str()
                        );
                    } else {
                        warn!(
                            "Invalid remark challenge for {}, received: {}, expected: {}",
                            identity.net_address.address_str(),
                            remark.as_str(),
                            identity.on_chain_challenge.as_str(),
                        )
                    }

                    // TODO: Notify web session
                }

                self.pending
                    .insert(identity.net_address, identity.on_chain_challenge);
            }
            EventType::RemarkFound(found) => {
                if let Some(challenge) = self.pending.get(&found.net_address) {
                    if challenge.matches_remark(&found) {
                        // TODO: Send judgement to watcher.
                        info!(
                            "Valid remark found for {}, submitting valid judgement",
                            found.net_address.address_str()
                        );
                    } else {
                        warn!(
                            "Invalid remark challenge for {}, received: {}, expected: {}",
                            found.net_address.address_str(),
                            found.remark.as_str(),
                            challenge.as_str(),
                        )
                    }

                    // TODO: Notify web session
                } else {
                    self.remarks.insert(found.net_address.clone(), found);
                }
            }
            _ => {}
        }

        Ok(())
    }
}
