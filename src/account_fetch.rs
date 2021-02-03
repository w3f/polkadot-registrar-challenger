use crate::manager::{IdentityState, NetworkAddress};
use crate::Result;
use frame_metadata::RuntimeMetadata;
use jsonrpc_client_transports::transports::ws::connect as ws_connect;
use jsonrpc_client_transports::{RawClient, TypedClient};
use jsonrpc_core::{MetaIoHandler, Params, Result as RpcResult, Value};
use jsonrpc_derive::rpc;
use parity_scale_codec::Decode;
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

        let client = ws_connect::<TypedClient>(&FromStr::from_str(url)?)
            .await
            .map_err(|_| anyhow!("Failed to connect to Substrate RPC"))?;

        let mut res = client
            .call_method::<Option<()>, String>("state_getMetadata", "", None)
            .await
            .map_err(|err| anyhow!("Failed to fetch metadata from Substrate RPC: {}", err))?;

        res.remove(0);
        res.remove(0);

        /*
        let mut decoded = hex::decode(res.as_bytes())?;
        let storage = RuntimeMetadata::decode(
            &mut decoded.as_slice()
        )?;
        */

        //println!(">> {}", String::from_utf8()?);

        Ok(())
    }
}

impl AccountFetch for SubstrateRpc {
    fn fetch_account_state(_net_address: &NetworkAddress) -> Result<Option<IdentityState>> {
        unimplemented!()
    }
}

#[test]
fn substrate_rpc_fetch_metadata() {
    tokio_compat::run_std(async {
        let _ = SubstrateRpc::fetch_metadata().await.unwrap();
    });
}

#[tokio::test]
async fn tester() {
    assert_eq!(1, 1);
}
