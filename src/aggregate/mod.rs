use eventually::store::{AppendError, EventStream, Expected, Persisted, Select};
use futures::future::BoxFuture;
use std::convert::{AsRef, TryFrom};
use std::marker::PhantomData;

pub mod message_watcher;
pub mod verifier;
