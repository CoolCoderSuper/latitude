use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};

use crate::config::DeploymentShareConfig;

use super::{CatalogStore, StorageError, encode_hex};

impl CatalogStore {
    pub(crate) async fn list_shares(&self) -> Result<Vec<DeploymentShareConfig>, StorageError> {
        let rows = sqlx::query(
            "SELECT token, project_name, deployment_name, password, expires_at \
             FROM share_links ORDER BY rowid",
        )
        .fetch_all(&self.inner.pool)
        .await?;
        rows.iter().map(share_from_row).collect()
    }

    pub(crate) async fn get_share(
        &self,
        token: &str,
    ) -> Result<Option<DeploymentShareConfig>, StorageError> {
        let row = sqlx::query(
            "SELECT token, project_name, deployment_name, password, expires_at \
             FROM share_links WHERE token = ?1",
        )
        .bind(token)
        .fetch_optional(&self.inner.pool)
        .await?;
        row.as_ref().map(share_from_row).transpose()
    }

    pub(crate) async fn create_share(
        &self,
        project: &str,
        deployment: &str,
        password: Option<String>,
        expires_at: Option<u64>,
    ) -> Result<DeploymentShareConfig, StorageError> {
        if self.get_deployment(project, deployment).await?.is_none() {
            return Err(StorageError::Invalid(format!(
                "deployment '{deployment}' was not found in project '{project}'"
            )));
        }

        let share = DeploymentShareConfig {
            token: self.generate_share_token().await?,
            project: project.to_string(),
            deployment: deployment.to_string(),
            password: password.filter(|password| !password.is_empty()),
            expires_at,
        };
        share.validate()?;
        insert_share(&self.inner.pool, &share).await?;
        Ok(share)
    }

    pub(crate) async fn delete_share(&self, token: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM share_links WHERE token = ?1")
            .bind(token)
            .execute(&self.inner.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn generate_share_token(&self) -> Result<String, StorageError> {
        loop {
            let token = encode_hex(rand::random::<[u8; 16]>());
            if self.get_share(&token).await?.is_none() {
                return Ok(token);
            }
        }
    }
}

async fn insert_share(
    pool: &sqlx::SqlitePool,
    share: &DeploymentShareConfig,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO share_links (token, project_name, deployment_name, password, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&share.token)
    .bind(&share.project)
    .bind(&share.deployment)
    .bind(&share.password)
    .bind(share.expires_at.map(|value| value as i64))
    .execute(pool)
    .await?;
    Ok(())
}

pub(super) async fn insert_share_tx(
    tx: &mut Transaction<'_, Sqlite>,
    share: &DeploymentShareConfig,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT OR REPLACE INTO share_links \
         (token, project_name, deployment_name, password, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&share.token)
    .bind(&share.project)
    .bind(&share.deployment)
    .bind(&share.password)
    .bind(share.expires_at.map(|value| value as i64))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn share_from_row(row: &SqliteRow) -> Result<DeploymentShareConfig, StorageError> {
    Ok(DeploymentShareConfig {
        token: row.try_get("token")?,
        project: row.try_get("project_name")?,
        deployment: row.try_get("deployment_name")?,
        password: row.try_get("password")?,
        expires_at: row
            .try_get::<Option<i64>, _>("expires_at")?
            .map(|value| value as u64),
    })
}
