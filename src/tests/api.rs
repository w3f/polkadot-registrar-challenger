use super::ApiBackend;
use crate::manager::IdentityState;
use jsonrpc_core::{Params, Value};

#[tokio_02::test]
async fn api_service() {
    env_logger::init();

    let be = ApiBackend::run().await;

    let alice = IdentityState::alice();

    let client = be.client();
    let _stream = client
        .subscribe(
            "account_subscribeStatus",
            Params::Array(vec![
                Value::String(alice.net_address.net_str().to_string()),
                Value::String(alice.net_address.address_str().to_string()),
            ]),
            "account_status",
            "account_unsubscribeStatus",
        )
        .unwrap();
}

/*
{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus"}

{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus","params":["polkadot","1gfpAmeKYhEoSrEgQ5UDYTiNSeKPvxVfLVWcW73JGnX9L6M"]}
*/
