use futures::future;
use jsonrpc_core::{MetaIoHandler, Params, Value};
use jsonrpc_pubsub::{PubSubHandler, Session, Subscriber, SubscriptionId};
use std::sync::Arc;

pub fn start_api() {
    let mut io = PubSubHandler::new(MetaIoHandler::default());
    io.add_subscription(
        "account_status",
        (
            "account_subscribeStatus",
            move |params: Params, _: Arc<Session>, subscriber: Subscriber| {},
        ),
        ("account_unsubscribeStatus", move |id: SubscriptionId, _| {
            future::ok(Value::Null)
        }),
    )
}
