mod shares;
mod worktrees;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use sha2::{Digest, Sha256};
use sqlx::{
    Row, Sqlite, SqlitePool, Transaction,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow},
};
use thiserror::Error;
use tokio::fs;
use tracing::warn;

use crate::config::{ApplicationConfig, ApplicationTarget, ConfigError, PageFormat, ProjectConfig};
use crate::util::encode_hex;

pub(crate) use worktrees::{DiscoveredWorktree, WorktreeRecord};

const DB_FILE_NAME: &str = "latitude.db";
const CONTENT_DIR_NAME: &str = "content";

#[derive(Clone)]
pub(crate) struct CatalogStore {
    inner: Arc<CatalogStoreInner>,
}

struct CatalogStoreInner {
    pool: SqlitePool,
    data_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct CatalogCounts {
    pub project_count: usize,
    pub deployment_count: usize,
    pub share_link_count: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct PageContent {
    pub bytes: Vec<u8>,
    pub format: PageFormat,
    pub media_type: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Error)]
pub(crate) enum StorageError {
    #[error("database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("content file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("catalog item is invalid: {0}")]
    Invalid(String),
}

struct PreparedDeployment {
    app: ApplicationConfig,
    content: Option<ContentMeta>,
}

#[derive(Clone, Debug)]
struct ContentMeta {
    path: String,
    hash: String,
    length: i64,
}

impl From<ConfigError> for StorageError {
    fn from(error: ConfigError) -> Self {
        Self::Invalid(error.to_string())
    }
}

impl CatalogStore {
    pub(crate) async fn open(data_dir: PathBuf) -> Result<Self, StorageError> {
        fs::create_dir_all(&data_dir).await?;
        fs::create_dir_all(data_dir.join(CONTENT_DIR_NAME)).await?;

        let options = SqliteConnectOptions::new()
            .filename(data_dir.join(DB_FILE_NAME))
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self {
            inner: Arc::new(CatalogStoreInner { pool, data_dir }),
        })
    }

    #[cfg(test)]
    pub(crate) async fn open_for_tests(data_dir: PathBuf) -> Result<Self, StorageError> {
        Self::open(data_dir).await
    }

    #[cfg(test)]
    pub(crate) fn data_dir(&self) -> &Path {
        &self.inner.data_dir
    }

    pub(crate) async fn counts(&self) -> Result<CatalogCounts, StorageError> {
        let project_count = count_table(&self.inner.pool, "projects").await?;
        let deployment_count = count_table(&self.inner.pool, "deployments").await?;
        let share_link_count = count_table(&self.inner.pool, "share_links").await?;
        Ok(CatalogCounts {
            project_count,
            deployment_count,
            share_link_count,
        })
    }

    pub(crate) async fn list_projects(&self) -> Result<Vec<ProjectConfig>, StorageError> {
        let rows = sqlx::query("SELECT name, enabled, project_dir FROM projects ORDER BY rowid")
            .fetch_all(&self.inner.pool)
            .await?;
        let mut projects = Vec::with_capacity(rows.len());
        for row in rows {
            let mut project = project_from_row(&row)?;
            project.deployments = self.list_project_deployments(&project.name).await?;
            projects.push(project);
        }
        Ok(projects)
    }

    pub(crate) async fn get_project(
        &self,
        name: &str,
    ) -> Result<Option<ProjectConfig>, StorageError> {
        let Some(row) =
            sqlx::query("SELECT name, enabled, project_dir FROM projects WHERE name = ?1")
                .bind(name)
                .fetch_optional(&self.inner.pool)
                .await?
        else {
            return Ok(None);
        };
        let mut project = project_from_row(&row)?;
        project.deployments = self.list_project_deployments(&project.name).await?;
        Ok(Some(project))
    }

    pub(crate) async fn create_project(&self, project: ProjectConfig) -> Result<(), StorageError> {
        project.validate()?;
        if self.get_project(&project.name).await?.is_some() {
            return Err(StorageError::Invalid(format!(
                "project '{}' already exists",
                project.name
            )));
        }
        let prepared = self.prepare_deployments(&project.deployments)?;
        let mut tx = self.inner.pool.begin().await?;
        insert_project_tx(&mut tx, &project).await?;
        for deployment in prepared {
            insert_deployment_tx(&mut tx, &project.name, &deployment).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn replace_project(&self, project: ProjectConfig) -> Result<(), StorageError> {
        project.validate()?;
        let old_content_paths = self.project_content_paths(&project.name).await?;
        let prepared = self.prepare_deployments(&project.deployments)?;
        let mut tx = self.inner.pool.begin().await?;
        sqlx::query("DELETE FROM projects WHERE name = ?1")
            .bind(&project.name)
            .execute(&mut *tx)
            .await?;
        insert_project_tx(&mut tx, &project).await?;
        for deployment in prepared {
            insert_deployment_tx(&mut tx, &project.name, &deployment).await?;
        }
        tx.commit().await?;
        self.prune_content_paths(old_content_paths).await?;
        Ok(())
    }

    pub(crate) async fn delete_project(&self, name: &str) -> Result<bool, StorageError> {
        let old_content_paths = self.project_content_paths(name).await?;
        let result = sqlx::query("DELETE FROM projects WHERE name = ?1")
            .bind(name)
            .execute(&self.inner.pool)
            .await?;
        let removed = result.rows_affected() > 0;
        if removed {
            self.prune_content_paths(old_content_paths).await?;
        }
        Ok(removed)
    }

    pub(crate) async fn list_project_deployments(
        &self,
        project: &str,
    ) -> Result<Vec<ApplicationConfig>, StorageError> {
        let rows = sqlx::query("SELECT * FROM deployments WHERE project_name = ?1 ORDER BY rowid")
            .bind(project)
            .fetch_all(&self.inner.pool)
            .await?;
        rows.iter()
            .map(deployment_from_row)
            .collect::<Result<Vec<_>, _>>()
    }

    pub(crate) async fn create_deployment(
        &self,
        project: &str,
        deployment: ApplicationConfig,
    ) -> Result<(), StorageError> {
        if self.get_project(project).await?.is_none() {
            return Err(StorageError::Invalid(format!(
                "project '{project}' was not found"
            )));
        }
        if self
            .get_deployment(project, &deployment.name)
            .await?
            .is_some()
        {
            return Err(StorageError::Invalid(format!(
                "deployment '{}' already exists in project '{}'",
                deployment.name, project
            )));
        }
        self.replace_deployment(project, deployment).await
    }

    pub(crate) async fn get_deployment(
        &self,
        project: &str,
        name: &str,
    ) -> Result<Option<ApplicationConfig>, StorageError> {
        let row = sqlx::query("SELECT * FROM deployments WHERE project_name = ?1 AND name = ?2")
            .bind(project)
            .bind(name)
            .fetch_optional(&self.inner.pool)
            .await?;
        row.as_ref().map(deployment_from_row).transpose()
    }

    pub(crate) async fn replace_deployment(
        &self,
        project: &str,
        deployment: ApplicationConfig,
    ) -> Result<(), StorageError> {
        deployment.validate()?;
        if self.get_project(project).await?.is_none() {
            return Err(StorageError::Invalid(format!(
                "project '{project}' was not found"
            )));
        }
        let old_content_paths = self
            .deployment_content_paths(project, &deployment.name)
            .await?;
        let prepared = self.prepare_deployment(&deployment)?;
        let mut tx = self.inner.pool.begin().await?;
        insert_deployment_tx(&mut tx, project, &prepared).await?;
        tx.commit().await?;
        self.prune_content_paths(old_content_paths).await?;
        Ok(())
    }

    pub(crate) async fn delete_deployment(
        &self,
        project: &str,
        name: &str,
    ) -> Result<bool, StorageError> {
        let old_content_paths = self.deployment_content_paths(project, name).await?;
        let result = sqlx::query("DELETE FROM deployments WHERE project_name = ?1 AND name = ?2")
            .bind(project)
            .bind(name)
            .execute(&self.inner.pool)
            .await?;
        let removed = result.rows_affected() > 0;
        if removed {
            self.prune_content_paths(old_content_paths).await?;
        }
        Ok(removed)
    }

    pub(crate) async fn set_deployment_archived(
        &self,
        project: &str,
        name: &str,
        archived: bool,
    ) -> Result<Option<ApplicationConfig>, StorageError> {
        let result = sqlx::query(
            "UPDATE deployments SET enabled = ?1 WHERE project_name = ?2 AND name = ?3",
        )
        .bind(bool_to_i64(!archived))
        .bind(project)
        .bind(name)
        .execute(&self.inner.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        self.get_deployment(project, name).await
    }

    pub(crate) async fn upsert_page(
        &self,
        project: &str,
        name: &str,
        format: PageFormat,
        media_type: Option<String>,
        title: Option<String>,
        bytes: Vec<u8>,
    ) -> Result<ApplicationConfig, StorageError> {
        if format != PageFormat::Binary {
            std::str::from_utf8(&bytes).map_err(|error| {
                StorageError::Invalid(format!("page content must be UTF-8 text: {error}"))
            })?;
        }

        let deployment = ApplicationConfig {
            name: name.to_string(),
            enabled: true,
            target: ApplicationTarget::Page {
                format,
                media_type,
                title,
            },
        };
        deployment.validate()?;
        if self.get_project(project).await?.is_none() {
            return Err(StorageError::Invalid(format!(
                "project '{project}' was not found"
            )));
        }

        let old_content_paths = self.deployment_content_paths(project, name).await?;
        let content = self.write_content_file(&bytes).await?;
        let prepared = PreparedDeployment {
            app: deployment,
            content: Some(content),
        };
        let mut tx = self.inner.pool.begin().await?;
        insert_deployment_tx(&mut tx, project, &prepared).await?;
        tx.commit().await?;
        self.prune_content_paths(old_content_paths).await?;

        self.get_deployment(project, name)
            .await?
            .ok_or_else(|| StorageError::Invalid("page deployment was not stored".to_string()))
    }

    pub(crate) async fn get_page_content(
        &self,
        project: &str,
        name: &str,
    ) -> Result<Option<PageContent>, StorageError> {
        let Some(row) = sqlx::query(
            "SELECT page_format, media_type, title, content_path FROM deployments \
             WHERE project_name = ?1 AND name = ?2 AND kind = 'page'",
        )
        .bind(project)
        .bind(name)
        .fetch_optional(&self.inner.pool)
        .await?
        else {
            return Ok(None);
        };

        let Some(content_path) = row.try_get::<Option<String>, _>("content_path")? else {
            return Ok(None);
        };
        let bytes = fs::read(self.inner.data_dir.join(content_path)).await?;
        Ok(Some(PageContent {
            bytes,
            format: page_format_from_db(&row.try_get::<String, _>("page_format")?)?,
            media_type: row.try_get("media_type")?,
            title: row.try_get("title")?,
        }))
    }

    fn prepare_deployments(
        &self,
        deployments: &[ApplicationConfig],
    ) -> Result<Vec<PreparedDeployment>, StorageError> {
        let mut prepared = Vec::with_capacity(deployments.len());
        for deployment in deployments {
            prepared.push(self.prepare_deployment(deployment)?);
        }
        Ok(prepared)
    }

    fn prepare_deployment(
        &self,
        deployment: &ApplicationConfig,
    ) -> Result<PreparedDeployment, StorageError> {
        deployment.validate()?;
        let content = match &deployment.target {
            ApplicationTarget::Page { .. } => {
                return Err(StorageError::Invalid(
                    "page deployments must be published through the page content endpoint"
                        .to_string(),
                ));
            }
            ApplicationTarget::ReverseProxy { .. } | ApplicationTarget::Static { .. } => None,
        };

        Ok(PreparedDeployment {
            app: deployment.clone(),
            content,
        })
    }

    async fn write_content_file(&self, bytes: &[u8]) -> Result<ContentMeta, StorageError> {
        let hash = sha256_hex(bytes);
        let relative = content_relative_path(&hash);
        let target = self.inner.data_dir.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await?;
        }

        if fs::metadata(&target).await.is_err() {
            let temp = target.with_extension(format!("tmp-{}", rand::random::<u64>()));
            fs::write(&temp, bytes).await?;
            match fs::rename(&temp, &target).await {
                Ok(()) => {}
                Err(error) if target.exists() => {
                    let _ = fs::remove_file(&temp).await;
                    if error.kind() != std::io::ErrorKind::AlreadyExists {
                        warn!(%error, path = %target.display(), "content file appeared during write");
                    }
                }
                Err(error) => return Err(error.into()),
            }
        }

        Ok(ContentMeta {
            path: relative,
            hash,
            length: bytes.len() as i64,
        })
    }

    async fn deployment_content_paths(
        &self,
        project: &str,
        name: &str,
    ) -> Result<Vec<String>, StorageError> {
        let rows = sqlx::query(
            "SELECT content_path FROM deployments \
             WHERE project_name = ?1 AND name = ?2 AND content_path IS NOT NULL",
        )
        .bind(project)
        .bind(name)
        .fetch_all(&self.inner.pool)
        .await?;
        rows.into_iter()
            .map(|row| row.try_get("content_path").map_err(StorageError::from))
            .collect()
    }

    async fn project_content_paths(&self, project: &str) -> Result<Vec<String>, StorageError> {
        let rows = sqlx::query(
            "SELECT content_path FROM deployments \
             WHERE project_name = ?1 AND content_path IS NOT NULL",
        )
        .bind(project)
        .fetch_all(&self.inner.pool)
        .await?;
        rows.into_iter()
            .map(|row| row.try_get("content_path").map_err(StorageError::from))
            .collect()
    }

    async fn prune_content_paths(&self, paths: Vec<String>) -> Result<(), StorageError> {
        for path in paths {
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM deployments WHERE content_path = ?1",
            )
            .bind(&path)
            .fetch_one(&self.inner.pool)
            .await?;
            if count == 0 {
                let full_path = self.inner.data_dir.join(&path);
                match fs::remove_file(&full_path).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(())
    }
}

async fn count_table(pool: &SqlitePool, table: &str) -> Result<usize, StorageError> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    let count = sqlx::query_scalar::<_, i64>(&sql).fetch_one(pool).await?;
    Ok(count as usize)
}

async fn insert_project_tx(
    tx: &mut Transaction<'_, Sqlite>,
    project: &ProjectConfig,
) -> Result<(), StorageError> {
    sqlx::query("INSERT INTO projects (name, enabled, project_dir) VALUES (?1, ?2, ?3)")
        .bind(&project.name)
        .bind(bool_to_i64(project.enabled))
        .bind(path_to_db(&project.project_dir))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_deployment_tx(
    tx: &mut Transaction<'_, Sqlite>,
    project: &str,
    deployment: &PreparedDeployment,
) -> Result<(), StorageError> {
    let app = &deployment.app;
    let (
        kind,
        upstream,
        strip_prefix,
        static_root,
        index_file,
        spa_fallback,
        page_format,
        media_type,
        title,
    ) = match &app.target {
        ApplicationTarget::ReverseProxy {
            upstream,
            strip_prefix,
        } => (
            "reverse_proxy",
            Some(upstream.clone()),
            Some(bool_to_i64(*strip_prefix)),
            None,
            None,
            None,
            None,
            None,
            None,
        ),
        ApplicationTarget::Static {
            root,
            index_file,
            spa_fallback,
        } => (
            "static",
            None,
            None,
            Some(path_to_db(root)),
            Some(index_file.clone()),
            Some(bool_to_i64(*spa_fallback)),
            None,
            None,
            None,
        ),
        ApplicationTarget::Page {
            format,
            media_type,
            title,
        } => (
            "page",
            None,
            None,
            None,
            None,
            None,
            Some(page_format_to_db(*format).to_string()),
            media_type.clone(),
            title.clone(),
        ),
    };
    let content = deployment.content.as_ref();

    sqlx::query(
        "INSERT INTO deployments (
            project_name, name, enabled, kind, upstream, strip_prefix, static_root, index_file,
            spa_fallback, page_format, media_type, title, content_path, content_hash, content_length
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        ON CONFLICT(project_name, name) DO UPDATE SET
            enabled = excluded.enabled,
            kind = excluded.kind,
            upstream = excluded.upstream,
            strip_prefix = excluded.strip_prefix,
            static_root = excluded.static_root,
            index_file = excluded.index_file,
            spa_fallback = excluded.spa_fallback,
            page_format = excluded.page_format,
            media_type = excluded.media_type,
            title = excluded.title,
            content_path = excluded.content_path,
            content_hash = excluded.content_hash,
            content_length = excluded.content_length",
    )
    .bind(project)
    .bind(&app.name)
    .bind(bool_to_i64(app.enabled))
    .bind(kind)
    .bind(upstream)
    .bind(strip_prefix)
    .bind(static_root)
    .bind(index_file)
    .bind(spa_fallback)
    .bind(page_format)
    .bind(media_type)
    .bind(title)
    .bind(content.map(|content| content.path.clone()))
    .bind(content.map(|content| content.hash.clone()))
    .bind(content.map(|content| content.length))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn project_from_row(row: &SqliteRow) -> Result<ProjectConfig, StorageError> {
    Ok(ProjectConfig {
        name: row.try_get("name")?,
        enabled: i64_to_bool(row.try_get("enabled")?),
        project_dir: PathBuf::from(row.try_get::<String, _>("project_dir")?),
        deployments: Vec::new(),
    })
}

fn deployment_from_row(row: &SqliteRow) -> Result<ApplicationConfig, StorageError> {
    let kind: String = row.try_get("kind")?;
    let target = match kind.as_str() {
        "reverse_proxy" => ApplicationTarget::ReverseProxy {
            upstream: row.try_get("upstream")?,
            strip_prefix: i64_to_bool(row.try_get("strip_prefix")?),
        },
        "static" => ApplicationTarget::Static {
            root: PathBuf::from(row.try_get::<String, _>("static_root")?),
            index_file: row.try_get("index_file")?,
            spa_fallback: i64_to_bool(row.try_get("spa_fallback")?),
        },
        "page" => ApplicationTarget::Page {
            format: page_format_from_db(&row.try_get::<String, _>("page_format")?)?,
            media_type: row.try_get("media_type")?,
            title: row.try_get("title")?,
        },
        _ => {
            return Err(StorageError::Invalid(format!(
                "unknown deployment kind '{kind}'"
            )));
        }
    };

    Ok(ApplicationConfig {
        name: row.try_get("name")?,
        enabled: i64_to_bool(row.try_get("enabled")?),
        target,
    })
}

fn path_to_db(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn bool_to_i64(value: bool) -> i64 {
    i64::from(value)
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}

fn page_format_to_db(format: PageFormat) -> &'static str {
    match format {
        PageFormat::Html => "html",
        PageFormat::Markdown => "markdown",
        PageFormat::Binary => "binary",
    }
}

fn page_format_from_db(value: &str) -> Result<PageFormat, StorageError> {
    match value {
        "html" => Ok(PageFormat::Html),
        "markdown" => Ok(PageFormat::Markdown),
        "binary" => Ok(PageFormat::Binary),
        _ => Err(StorageError::Invalid(format!(
            "unknown page format '{value}'"
        ))),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    encode_hex(Sha256::digest(bytes))
}

fn content_relative_path(hash: &str) -> String {
    format!("{CONTENT_DIR_NAME}/{}/{}", &hash[..2], hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_data_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "latitude-storage-test-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn page_deployment_metadata_omits_content_but_content_endpoint_reads_bytes() {
        let store = CatalogStore::open_for_tests(temp_data_dir("metadata"))
            .await
            .unwrap();
        store
            .create_project(ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: Vec::new(),
            })
            .await
            .unwrap();

        let deployment = store
            .upsert_page(
                "demo",
                "report",
                PageFormat::Markdown,
                None,
                Some("Report".to_string()),
                b"# Report".to_vec(),
            )
            .await
            .unwrap();
        let json = serde_json::to_value(&deployment).unwrap();

        assert_eq!(json["kind"], "page");
        assert_eq!(json["title"], "Report");
        assert!(json.get("content").is_none());
        assert_eq!(
            store
                .get_page_content("demo", "report")
                .await
                .unwrap()
                .unwrap()
                .bytes,
            b"# Report"
        );
    }

    #[tokio::test]
    async fn archives_and_restores_deployment_without_removing_page_content() {
        let store = CatalogStore::open_for_tests(temp_data_dir("deployment-archive"))
            .await
            .unwrap();
        store
            .create_project(ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: Vec::new(),
            })
            .await
            .unwrap();
        store
            .upsert_page(
                "demo",
                "report",
                PageFormat::Markdown,
                None,
                Some("Report".to_string()),
                b"# Report".to_vec(),
            )
            .await
            .unwrap();

        let archived = store
            .set_deployment_archived("demo", "report", true)
            .await
            .unwrap()
            .unwrap();
        assert!(!archived.enabled);
        assert_eq!(
            store
                .get_page_content("demo", "report")
                .await
                .unwrap()
                .unwrap()
                .bytes,
            b"# Report"
        );

        let restored = store
            .set_deployment_archived("demo", "report", false)
            .await
            .unwrap()
            .unwrap();
        assert!(restored.enabled);
        assert!(
            store
                .set_deployment_archived("demo", "missing", true)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn reconciles_archives_and_removes_discovered_worktrees() {
        let data_dir = temp_data_dir("worktrees");
        let primary_dir = data_dir.join("repo");
        let linked_dir = data_dir.join("repo-fix");
        std::fs::create_dir_all(&primary_dir).unwrap();
        std::fs::create_dir_all(&linked_dir).unwrap();
        let store = CatalogStore::open_for_tests(data_dir.clone())
            .await
            .unwrap();
        let primary = ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: primary_dir.clone(),
            deployments: Vec::new(),
        };
        store.create_project(primary.clone()).await.unwrap();
        let common_dir = data_dir.join("common.git");
        let worktrees = vec![
            DiscoveredWorktree {
                worktree_dir: primary_dir,
                branch: Some("master".to_string()),
                head: "abc123".to_string(),
            },
            DiscoveredWorktree {
                worktree_dir: linked_dir,
                branch: Some("codex/fix".to_string()),
                head: "def456".to_string(),
            },
        ];

        store
            .reconcile_worktrees(&common_dir, &primary, &worktrees)
            .await
            .unwrap();
        let records = store.list_worktrees().await.unwrap();
        assert_eq!(records.len(), 2);
        let root = records.iter().find(|record| !record.discovered).unwrap();
        assert!(
            !store
                .set_worktree_archived(&root.project_name, true)
                .await
                .unwrap()
        );
        let linked = records.iter().find(|record| record.discovered).unwrap();
        assert_eq!(linked.branch.as_deref(), Some("codex/fix"));
        assert!(
            store
                .set_worktree_archived(&linked.project_name, true)
                .await
                .unwrap()
        );
        store
            .reconcile_worktrees(&common_dir, &primary, &worktrees)
            .await
            .unwrap();
        assert!(
            store
                .list_worktrees()
                .await
                .unwrap()
                .iter()
                .find(|record| record.discovered)
                .unwrap()
                .archived
        );

        store
            .reconcile_worktrees(&common_dir, &primary, &worktrees[..1])
            .await
            .unwrap();
        let records = store.list_worktrees().await.unwrap();
        assert_eq!(records.len(), 1);
        assert!(!records[0].discovered);
        assert_eq!(store.list_projects().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn replacing_and_deleting_pages_prunes_unreferenced_content_files() {
        let store = CatalogStore::open_for_tests(temp_data_dir("prune"))
            .await
            .unwrap();
        store
            .create_project(ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: Vec::new(),
            })
            .await
            .unwrap();

        store
            .upsert_page(
                "demo",
                "report",
                PageFormat::Markdown,
                None,
                None,
                b"first".to_vec(),
            )
            .await
            .unwrap();
        assert_eq!(content_file_count(store.data_dir()), 1);

        store
            .upsert_page(
                "demo",
                "report",
                PageFormat::Markdown,
                None,
                None,
                b"second".to_vec(),
            )
            .await
            .unwrap();
        assert_eq!(content_file_count(store.data_dir()), 1);
        assert_eq!(
            store
                .get_page_content("demo", "report")
                .await
                .unwrap()
                .unwrap()
                .bytes,
            b"second"
        );

        assert!(store.delete_deployment("demo", "report").await.unwrap());
        assert_eq!(content_file_count(store.data_dir()), 0);
    }

    fn content_file_count(data_dir: &Path) -> usize {
        let content_dir = data_dir.join(CONTENT_DIR_NAME);
        if !content_dir.exists() {
            return 0;
        }
        count_files(&content_dir)
    }

    fn count_files(path: &Path) -> usize {
        let mut count = 0;
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                count += count_files(&path);
            } else {
                count += 1;
            }
        }
        count
    }
}
