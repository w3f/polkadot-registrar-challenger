use super::JsonResult;
use crate::actors::connector::DisplayNameEntry;
use crate::database::Database;
use crate::primitives::IdentityFieldValue;
use actix::prelude::*;
use actix_web::{web, HttpResponse};

pub struct DisplayNameChecker {
	db: Database,
}

impl Default for DisplayNameChecker {
	fn default() -> Self {
		panic!("DisplayNameChecker is not initialized");
	}
}

impl DisplayNameChecker {
	pub fn new(db: Database) -> Self {
		DisplayNameChecker { db: db }
	}
}

impl SystemService for DisplayNameChecker {}
impl Supervised for DisplayNameChecker {}

impl Actor for DisplayNameChecker {
    type Context = Context<Self>;
}

impl Handler<CheckDisplayName> for DisplayNameChecker {
    type Result = ResponseActFuture<Self, JsonResult<bool>>;

    fn handle(&mut self, msg: CheckDisplayName, _ctx: &mut Self::Context) -> Self::Result {
        let mut db = self.db.clone();

		unimplemented!()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Message)]
#[rtype(result = "JsonResult<bool>")]
pub struct CheckDisplayName {
    pub display_name: DisplayNameEntry,
}
