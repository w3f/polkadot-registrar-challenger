use actix::prelude::*;

pub mod lookup_server;
pub mod session;

#[derive(Debug, Clone, Serialize, MessageResponse)]
#[serde(rename_all = "snake_case", tag = "result_type", content = "message")]
// TODO: Rename this
pub enum JsonResult<T, E> {
    Ok(T),
    Err(E),
}
