use crate::Result;
use crate::primitives::{ChainAddress, ChainRemark, JudgementState, ExternalMessage};

pub enum VerifyOutcome {
    FieldOk,
    FullyVerified,
}

pub struct Database {

}

impl Database {
    pub fn new() -> Self {
        Database {

        }
    }
    pub fn add_judgement_request(&self, request: JudgementState) -> Result<()> {
        unimplemented!()
    }
    pub fn add_message(&self) -> Result<()> {
        unimplemented!()
    }
    pub fn add_chain_remark(&self, remark: ChainRemark) -> Result<()> {
        unimplemented!()
    }
    pub fn verify_message(&self, message: ExternalMessage) -> Result<VerifyOutcome> {
        unimplemented!()
    }
    pub fn check_state_change(&self, chain_address: ChainAddress) -> Result<JudgementState> {
        unimplemented!()
    }
}
