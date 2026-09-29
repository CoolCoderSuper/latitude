use std::path::{Path, PathBuf};

use futures_util::{StreamExt, stream};

use super::{
    command::run_git_command, diff::collect_repository_view_status, types::GitStatusSummary,
};

#[derive(Clone, Debug)]
pub(in crate::server) struct GitRepositorySummary {
    pub path: String,
    pub status: GitStatusSummary,
}

pub(in crate::server) async fn submodule_paths(root: &Path) -> Result<Vec<String>, String> {
    let output = run_git_command(
        root,
        &[
            "submodule",
            "foreach",
            "--quiet",
            "--recursive",
            "printf '%s\\0' \"$displaypath\"",
        ],
        &[0],
    )
    .await?;
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect())
}

pub(in crate::server) fn resolve_repository(
    root: &Path,
    paths: &[String],
    selected: &str,
) -> Result<PathBuf, String> {
    if selected.is_empty() {
        return Ok(root.to_path_buf());
    }
    if !paths.iter().any(|path| path == selected)
        || Path::new(selected)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("Unknown or uninitialized submodule".to_string());
    }
    Ok(root.join(selected))
}

pub(in crate::server) async fn repository_summaries(
    root: &Path,
    paths: &[String],
) -> Vec<GitRepositorySummary> {
    let repositories: Vec<_> = std::iter::once(String::new())
        .chain(paths.iter().cloned())
        .map(|path| (root.join(&path), path))
        .collect();
    stream::iter(repositories)
        .map(|(directory, path)| async move {
            GitRepositorySummary {
                status: collect_repository_view_status(&directory).await,
                path,
            }
        })
        .buffered(4)
        .collect()
        .await
}

pub(in crate::server) fn repository_url(base: &str, repository: &str) -> String {
    if repository.is_empty() {
        return base.to_string();
    }
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("repository", repository)
        .finish();
    format!(
        "{base}{}{query}",
        if base.contains('?') { '&' } else { '?' }
    )
}
