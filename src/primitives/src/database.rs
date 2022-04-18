use crate::types::{IdentityContext, IdentityState, Result};
use sqlx::{Pool, Postgres};

pub struct Database {
    db: Pool<Postgres>,
}

impl Database {
    pub async fn new(uri: &str) -> Result<Self> {
        unimplemented!()
    }
    async fn raw_identity_id(&self, identity: &IdentityContext) -> Result<Option<i32>> {
        sqlx::query!(
            "
            SELECT
                id
            FROM
                identity
            WHERE
                address = $1
            AND
                network_id = (
                    SELECT
                        id
                    FROM
                        network
                    WHERE
                        name = $2
                )
        ",
            // TODO: Should work without `as_str()`.
            identity.address.as_str(),
            identity.chain.as_str()
        )
        .fetch_optional(&self.db)
        .await
        .map_err(|err| anyhow!("failed retrieving identity: {:?}", err))
        .map(|try_row| try_row.map(|row| row.id))
    }
    pub async fn insert_judgement_state(&self, state: &IdentityState) -> Result<()> {
        let tx = self.db.begin().await?;

        let raw_id = self.raw_identity_id(&state.context).await?;

        let q1 = sqlx::query!(
            "
			SELECT
				id
			FROM
				judgement
		"
        );

        tx.commit().await?;
        Ok(())
    }
}
