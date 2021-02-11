use super::ApiBackend;
use crate::event::ErrorMessage;
use crate::manager::IdentityState;
use futures::StreamExt;
use jsonrpc_core::types::{to_value, Params, Value};

#[test]
fn subscribe_status_no_judgement_request() {
    let mut rt = tokio_02::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let be = ApiBackend::run().await;

        let alice = IdentityState::alice();

        #[rustfmt::skip]
        let expected = [
            to_value(ErrorMessage::no_pending_judgement_request(0)).unwrap(),
        ];

        let messages = be
            .get_messages(
                "account_subscribeStatus",
                Params::Array(vec![
                    Value::String(alice.net_address.net_str().to_string()),
                    Value::String(alice.net_address.address_str().to_string()),
                ]),
                "account_status",
                "account_unsubscribeStatus",
            )
            .await;

        assert_eq!(messages.len(), expected.len());
        for (message, expected) in messages.iter().zip(expected.iter()) {
            assert_eq!(message, expected)
        }
    });
}

/*
{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus"}

{"id":1,"jsonrpc":"2.0","method":"account_subscribeStatus","params":["polkadot","1gfpAmeKYhEoSrEgQ5UDYTiNSeKPvxVfLVWcW73JGnX9L6M"]}
*/
