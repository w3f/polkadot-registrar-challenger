use crate::manager::{IdentityState, NetworkAddress};
use crate::Result;
use jsonrpc_client_transports::{TypedClient};
use jsonrpc_client_transports::transports::ws::connect as ws_connect;
use jsonrpc_core::{MetaIoHandler, Params, Result as RpcResult, Value};
use jsonrpc_derive::rpc;
use std::str::FromStr;

pub trait AccountFetch {
    fn fetch_account_state(net_address: &NetworkAddress) -> Result<Option<IdentityState>>;
}

#[rpc]
pub trait RpcClient {
    #[rpc(name = "protocolVersion")]
    fn state_get_metadata(&self, s: Option<()>) -> RpcResult<String>;
}

pub struct SubstrateRpc {}

impl SubstrateRpc {
    async fn fetch_metadata() -> Result<()> {
        let url = "wss://registrar-test-0.w3f.tech";

        let _client = ws_connect::<TypedClient>(&FromStr::from_str(url)?)
            .await
            .map_err(|_| anyhow!("Failed to connect to Substrate RPC"))?;

        Ok(())
    }
}

impl AccountFetch for SubstrateRpc {
    fn fetch_account_state(_net_address: &NetworkAddress) -> Result<Option<IdentityState>> {
        unimplemented!()
    }
}
