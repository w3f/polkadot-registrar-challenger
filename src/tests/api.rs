use super::ApiBackend;
use crate::manager::IdentityState;
use futures::StreamExt;
use jsonrpc_core::{Params, Value};

#[test]
fn api_service() {
    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let be = ApiBackend::run().await;

        let alice = IdentityState::alice();

        let client = be.client();
        let mut stream = client
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

        while let Some(m) = stream.next().await {
            println!(">> {:?}", m.unwrap());
        }

        println!("GOT HERE");
    });
}

/*
{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus"}

{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus","params":["polkadot","1gfpAmeKYhEoSrEgQ5UDYTiNSeKPvxVfLVWcW73JGnX9L6M"]}
*/
