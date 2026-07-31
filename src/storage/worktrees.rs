use std::path::{Path, PathBuf};

use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};

use crate::config::ProjectConfig;

use super::{CatalogStore, StorageError, bool_to_i64, path_to_db, project_from_row};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorktreeRecord {
    pub project_name: String,
    pub common_git_dir: PathBuf,
    pub worktree_dir: PathBuf,
    pub branch: Option<String>,
    pub head: String,
    pub discovered: bool,
    pub archived: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DiscoveredWorktree {
    pub worktree_dir: PathBuf,
    pub branch: Option<String>,
    pub head: String,
}

impl CatalogStore {
    pub(crate) async fn list_worktrees(&self) -> Result<Vec<WorktreeRecord>, StorageError> {
        let rows = sqlx::query("SELECT * FROM worktrees ORDER BY rowid")
            .fetch_all(&self.inner.pool)
            .await?;
        rows.iter().map(worktree_from_row).collect()
    }

    pub(crate) async fn list_worktree_roots(&self) -> Result<Vec<ProjectConfig>, StorageError> {
        let rows = sqlx::query(
            "SELECT p.name, p.enabled, p.project_dir
             FROM projects p
             LEFT JOIN worktrees w ON w.project_name = p.name
             WHERE w.discovered IS NULL OR w.discovered = 0
             ORDER BY p.rowid",
        )
        .fetch_all(&self.inner.pool)
        .await?;
        rows.iter().map(project_from_row).collect()
    }

    pub(crate) async fn reconcile_worktrees(
        &self,
        common_git_dir: &Path,
        source_project: &ProjectConfig,
        discovered: &[DiscoveredWorktree],
    ) -> Result<(), StorageError> {
        let common_git_dir = path_to_db(common_git_dir);
        let source_dir = canonical_or_original(&source_project.project_dir);
        let mut tx = self.inner.pool.begin().await?;

        let existing_rows = sqlx::query(
            "SELECT project_name, worktree_dir FROM worktrees WHERE common_git_dir = ?1",
        )
        .bind(&common_git_dir)
        .fetch_all(&mut *tx)
        .await?;
        let existing = existing_rows
            .iter()
            .map(|row| {
                Ok((
                    PathBuf::from(row.try_get::<String, _>("worktree_dir")?),
                    row.try_get::<String, _>("project_name")?,
                ))
            })
            .collect::<Result<std::collections::HashMap<_, _>, sqlx::Error>>()?;

        let mut seen_paths = Vec::with_capacity(discovered.len());
        for worktree in discovered {
            let worktree_dir = canonical_or_original(&worktree.worktree_dir);
            let worktree_db = path_to_db(&worktree_dir);
            seen_paths.push(worktree_db.clone());

            let configured_name = if same_path(&source_dir, &worktree_dir) {
                Some(source_project.name.clone())
            } else {
                sqlx::query_scalar::<_, String>(
                    "SELECT p.name
                     FROM projects p
                     LEFT JOIN worktrees w ON w.project_name = p.name
                     WHERE lower(p.project_dir) = lower(?1)
                       AND (w.discovered IS NULL OR w.discovered = 0)
                     LIMIT 1",
                )
                .bind(&worktree_db)
                .fetch_optional(&mut *tx)
                .await?
            };
            let existing_name = existing
                .iter()
                .find(|(path, _)| same_path(path, &worktree_dir))
                .map(|(_, name)| name.clone());
            let (project_name, is_discovered) = if let Some(name) = configured_name {
                (name, false)
            } else if let Some(name) = existing_name {
                (name, true)
            } else {
                let name = unique_worktree_name_tx(
                    &mut tx,
                    &source_project.name,
                    worktree.branch.as_deref(),
                    &worktree_dir,
                )
                .await?;
                sqlx::query("INSERT INTO projects (name, enabled, project_dir) VALUES (?1, 1, ?2)")
                    .bind(&name)
                    .bind(&worktree_db)
                    .execute(&mut *tx)
                    .await?;
                (name, true)
            };

            sqlx::query(
                "INSERT INTO worktrees (
                    project_name, common_git_dir, worktree_dir, branch, head, discovered, archived
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
                 ON CONFLICT(project_name) DO UPDATE SET
                    common_git_dir = excluded.common_git_dir,
                    worktree_dir = excluded.worktree_dir,
                    branch = excluded.branch,
                    head = excluded.head,
                    discovered = excluded.discovered",
            )
            .bind(project_name)
            .bind(&common_git_dir)
            .bind(worktree_db)
            .bind(&worktree.branch)
            .bind(&worktree.head)
            .bind(bool_to_i64(is_discovered))
            .execute(&mut *tx)
            .await?;
        }

        let stale = sqlx::query(
            "SELECT project_name, worktree_dir FROM worktrees
             WHERE common_git_dir = ?1 AND discovered = 1",
        )
        .bind(&common_git_dir)
        .fetch_all(&mut *tx)
        .await?;
        for row in stale {
            let path: String = row.try_get("worktree_dir")?;
            if !seen_paths
                .iter()
                .any(|seen| same_path(Path::new(seen), Path::new(&path)))
            {
                let name: String = row.try_get("project_name")?;
                sqlx::query("DELETE FROM projects WHERE name = ?1")
                    .bind(name)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn set_worktree_archived(
        &self,
        project: &str,
        archived: bool,
    ) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE worktrees SET archived = ?1 WHERE project_name = ?2 AND discovered = 1",
        )
        .bind(bool_to_i64(archived))
        .bind(project)
        .execute(&self.inner.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn worktree_from_row(row: &SqliteRow) -> Result<WorktreeRecord, StorageError> {
    Ok(WorktreeRecord {
        project_name: row.try_get("project_name")?,
        common_git_dir: PathBuf::from(row.try_get::<String, _>("common_git_dir")?),
        worktree_dir: PathBuf::from(row.try_get::<String, _>("worktree_dir")?),
        branch: row.try_get("branch")?,
        head: row.try_get("head")?,
        discovered: row.try_get::<i64, _>("discovered")? != 0,
        archived: row.try_get::<i64, _>("archived")? != 0,
    })
}

fn canonical_or_original(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(left: &Path, right: &Path) -> bool {
    path_to_db(left).eq_ignore_ascii_case(&path_to_db(right))
}

async fn unique_worktree_name_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository_name: &str,
    branch: Option<&str>,
    path: &Path,
) -> Result<String, StorageError> {
    let label = branch
        .and_then(|branch| branch.rsplit('/').next())
        .filter(|branch| !branch.is_empty())
        .or_else(|| path.file_name().and_then(|name| name.to_str()))
        .unwrap_or("worktree");
    let slug = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let base = format!(
        "{repository_name}--{}",
        if slug.is_empty() { "worktree" } else { &slug }
    );
    let mut candidate = base.clone();
    let mut suffix = 2;
    while sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects WHERE name = ?1")
        .bind(&candidate)
        .fetch_one(&mut **tx)
        .await?
        > 0
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    Ok(candidate)
}
