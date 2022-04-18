use crate::actors::api::VerifyChallenge;
use crate::actors::connector::DisplayNameEntry;
use crate::adapters::admin::RawFieldName;
use crate::primitives::{
    ChainAddress, ChainName, ChallengeType, Event, ExpectedMessage, ExternalMessage,
    IdentityContext, IdentityFieldValue, JudgementState, NotificationMessage, Timestamp,
};
use crate::Result;

#[derive(Debug, Clone)]
pub struct Database {}

impl Database {
    pub async fn new(uri: &str, db: &str) -> Result<Self> {
        unimplemented!()
    }
    pub async fn add_judgement_request(&self, request: &JudgementState) -> Result<()> {
        unimplemented!()
    }
    #[cfg(test)]
    pub async fn delete_judgement(&self, context: &IdentityContext) -> Result<()> {
        unimplemented!()
    }
    pub async fn verify_manually(
        &self,
        context: &IdentityContext,
        field: &RawFieldName,
    ) -> Result<Option<()>> {
        unimplemented!()
    }
    pub async fn verify_message(&self, message: &ExternalMessage) -> Result<()> {
        unimplemented!()
    }
    /// Check if all fields have been verified.
    async fn process_fully_verified(&self, state: &JudgementState) -> Result<()> {
        unimplemented!()
    }
    pub async fn verify_second_challenge(&mut self, mut request: VerifyChallenge) -> Result<bool> {
        unimplemented!()
    }
    pub async fn fetch_second_challenge(
        &self,
        context: &IdentityContext,
        field: &IdentityFieldValue,
    ) -> Result<ExpectedMessage> {
        unimplemented!()
    }
    pub async fn fetch_events(
        &mut self,
        mut after: u64,
    ) -> Result<(Vec<NotificationMessage>, u64)> {
        unimplemented!()
    }
    pub async fn fetch_judgement_state(
        &self,
        context: &IdentityContext,
    ) -> Result<Option<JudgementState>> {
        unimplemented!()
    }
    pub async fn fetch_judgement_candidates(&self) -> Result<Vec<JudgementState>> {
        unimplemented!()
    }
    pub async fn set_judged(&self, context: &IdentityContext) -> Result<()> {
        unimplemented!()
    }
    pub async fn insert_display_name(&self, name: &DisplayNameEntry) -> Result<()> {
        unimplemented!()
    }
    pub async fn fetch_display_names(&self, chain: ChainName) -> Result<Vec<DisplayNameEntry>> {
        unimplemented!()
    }
    pub async fn set_display_name_valid(&self, state: &JudgementState) -> Result<()> {
        unimplemented!()
    }
    pub async fn insert_display_name_violations(
        &self,
        context: &IdentityContext,
        violations: &Vec<DisplayNameEntry>,
    ) -> Result<()> {
        unimplemented!()
    }
    async fn insert_event<T: Into<Event>>(&self, event: T) -> Result<()> {
        unimplemented!()
    }
    // TODO: Test this.
    pub async fn process_tangling_submissions(&self, addresses: &[&ChainAddress]) -> Result<()> {
        unimplemented!()
    }
}
