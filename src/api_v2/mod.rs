use crate::event::ErrorMessage;
use actix::prelude::*;

pub mod lookup_server;
pub mod session;

#[derive(Debug, Clone, Serialize, Message)]
#[rtype(result = "()")]
#[serde(rename_all = "snake_case", tag = "result_type", content = "message")]
// TODO: Rename this
enum JsonResult<T> {
    Ok(T),
    Err(ErrorMessage),
}
