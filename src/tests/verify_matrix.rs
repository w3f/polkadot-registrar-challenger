use super::db_path;
use super::mocks::*;
use crate::connector::{ConnectorWriterTransport, EventType, JudgementRequest, Message};
use crate::primitives::{Account, AccountType, NetAccount};
use crate::{test_run, Database2};
use matrix_sdk::identifiers::UserId;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::runtime::Runtime;

#[test]
fn verify_matrix() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let db = Database2::new(&db_path()).unwrap();
        let manager = Arc::new(EventManager2::new());
        let (_, matrix_child) = manager.child();

        let my_user_id = UserId::try_from("@registrar:matrix.org").unwrap();
        let matrix_transport = MatrixMocker::new(matrix_child, my_user_id);

        let handlers = test_run(
            Arc::clone(&manager),
            db,
            matrix_transport,
            DummyTransport::new(),
            DummyTransport::new(),
        )
        .await
        .unwrap();

        let mut connector = handlers.writer;
        let matrix = handlers.matrix;

        connector
            .write(&Message {
                event: EventType::NewJudgementRequest,
                data: serde_json::to_value(&JudgementRequest {
                    address: NetAccount::alice(),
                    accounts: [(
                        AccountType::Matrix,
                        Some(Account::from("@alice:matrix.org")),
                    )]
                    .iter()
                    .cloned()
                    .collect(),
                })
                .unwrap(),
            })
            .await
            .unwrap();
    });
}
