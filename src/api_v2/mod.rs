use actix::prelude::*;

pub mod lookup_server;
pub mod session;

#[derive(Debug, Clone, Serialize, Message)]
#[serde(rename_all = "snake_case", tag = "result_type", content = "message")]
#[rtype(result="()")]
// TODO: Rename this
pub enum JsonResult<T> {
    Ok(T),
    Err(String),
}
