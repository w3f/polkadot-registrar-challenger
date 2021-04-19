use crate::event::ErrorMessage;
use actix::prelude::*;

pub mod lookup_server;
pub mod session;

#[derive(Debug, Clone, Serialize, Message)]
#[rtype(result = "()")]
#[serde(untagged)]
enum MessageResult<T> {
    Ok(T),
    Err(ErrorMessage),
}
