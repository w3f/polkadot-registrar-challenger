use crate::manager::{IdentityState, NetworkAddress};
use crate::Result;
use std::sync::Weak;

pub trait AccountFetch {
    fn fetch_account_state(net_address: &NetworkAddress) -> Result<Option<IdentityState>>;
}
