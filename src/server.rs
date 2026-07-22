mod assets;
mod auth;
mod command;
mod constants;
mod desktop_api;
mod files_api;
mod git;
mod html;
mod page;
mod paths;
mod presentation;
mod public;
mod render;
mod response;
mod t3code;
mod terminal_api;

#[cfg(test)]
mod tests;

use axum::{
    Router,
    routing::{delete, get, post},
};
use tokio::net::TcpListener;
use tracing::info;

use crate::state::AppState;

pub(crate) use git::{GitDiffReport, GitStatusSummary};

use assets::{ASSET_BASE_PATH, public_asset};
use auth::open_t3code_embed;
use command::{
    command_health, create_deployment_share, create_project, create_project_deployment,
    create_t3code_embed_session, delete_deployment_share, delete_project,
    delete_project_deployment, get_config, get_deployment_share, get_project,
    get_project_deployment, get_project_page_content, list_deployment_shares,
    list_project_deployments, list_projects, put_config, replace_project,
    replace_project_deployment, upsert_project_page,
};
use constants::{
    LOGIN_PATH, PUBLIC_API_PROJECT_DIFF_PATH, PUBLIC_API_PROJECT_FILES_PATH,
    PUBLIC_API_PROJECT_GIT_COMMIT_PATH, PUBLIC_API_PROJECT_GIT_HISTORY_PATH,
    PUBLIC_API_PROJECT_PATH, PUBLIC_API_PROJECT_TERMINAL_PATH,
    PUBLIC_API_PROJECT_TERMINAL_SESSION_PATH, PUBLIC_API_PROJECT_TERMINAL_SESSIONS_PATH,
    PUBLIC_API_PROJECTS_PATH, PUBLIC_API_ROOT_DESKTOP_PATH, PUBLIC_API_ROOT_TERMINAL_PATH,
    PUBLIC_API_ROOT_TERMINAL_SESSION_PATH, PUBLIC_API_ROOT_TERMINAL_SESSIONS_PATH,
    PUBLIC_API_SESSION_PATH, PUBLIC_API_SHARE_PATH, PUBLIC_API_SHARES_PATH,
    PUBLIC_ROOT_DESKTOP_WS_PATH, PUBLIC_ROOT_TERMINAL_WS_PATH, PUBLIC_TERMINAL_WS_PATH,
    T3CODE_EMBED_PATH,
};
use desktop_api::{
    public_api_get_root_desktop, public_api_patch_root_desktop, public_root_desktop_ws,
};
use files_api::{
    public_api_get_project_files, public_api_highlight_project_file, public_api_put_project_file,
    public_ui_put_project_file,
};
use public::{
    get_public_login, post_public_login, public_api_create_root_terminal_session,
    public_api_create_share, public_api_create_terminal_session,
    public_api_delete_root_terminal_session, public_api_delete_share,
    public_api_delete_terminal_session, public_api_get_project, public_api_get_project_diff,
    public_api_get_project_git_commit, public_api_get_project_git_history,
    public_api_get_project_terminal, public_api_get_root_terminal, public_api_list_projects,
    public_api_list_root_terminal_sessions, public_api_list_shares,
    public_api_list_terminal_sessions, public_api_login, public_api_patch_project_archive,
    public_api_patch_project_diff, public_api_post_project_terminal, public_api_post_root_terminal,
    public_api_session, public_entry, public_root_terminal_ws, public_terminal_ws,
    public_ui_archive_project, public_ui_create_share, public_ui_delete_share,
    public_ui_get_shares,
};
use t3code::{open_project_in_t3code, open_t3code, t3code_gateway_router};

pub async fn run(state: AppState) -> anyhow::Result<()> {
    let config = state.config_snapshot().await;
    let public_bind = config.public_bind.clone();
    let command_bind = config.command_bind.clone();

    render::warm_syntax_highlighter();

    let public_listener = TcpListener::bind(&public_bind).await?;
    let command_listener = TcpListener::bind(&command_bind).await?;
    let gateway_listener = if config.t3code.enabled {
        match config.t3code.gateway_bind.as_deref() {
            Some(bind) => Some((bind.to_string(), TcpListener::bind(bind).await?)),
            None => None,
        }
    } else {
        None
    };

    info!(bind = %public_bind, "public proxy listening");
    info!(bind = %command_bind, "command API listening");

    let public_router = public_router(state.clone());
    let command_router = command_router(state.clone());

    if let Some((gateway_bind, gateway_listener)) = gateway_listener {
        info!(bind = %gateway_bind, "authenticated T3 Code gateway listening");
        let gateway_router = t3code_gateway_router(state);
        tokio::select! {
            result = axum::serve(public_listener, public_router) => { result?; }
            result = axum::serve(command_listener, command_router) => { result?; }
            result = axum::serve(gateway_listener, gateway_router) => { result?; }
        }
    } else {
        tokio::select! {
            result = axum::serve(public_listener, public_router) => { result?; }
            result = axum::serve(command_listener, command_router) => { result?; }
        }
    }

    Ok(())
}

fn public_router(state: AppState) -> Router {
    Router::new()
        .route(&format!("{ASSET_BASE_PATH}/{{name}}"), get(public_asset))
        .route(LOGIN_PATH, get(get_public_login).post(post_public_login))
        .route("/__latitude/t3code", get(open_t3code))
        .route("/__latitude/t3code/{project}", get(open_project_in_t3code))
        .route(T3CODE_EMBED_PATH, get(open_t3code_embed))
        .route(
            PUBLIC_API_SESSION_PATH,
            get(public_api_session).post(public_api_login),
        )
        .route(PUBLIC_API_PROJECTS_PATH, get(public_api_list_projects))
        .route(
            PUBLIC_API_SHARES_PATH,
            get(public_api_list_shares).post(public_api_create_share),
        )
        .route(PUBLIC_API_SHARE_PATH, delete(public_api_delete_share))
        .route(
            "/__latitude/ui/shares/{project}/{deployment}",
            get(public_ui_get_shares).post(public_ui_create_share),
        )
        .route(
            "/__latitude/ui/shares/{project}/{deployment}/{token}",
            delete(public_ui_delete_share),
        )
        .route(
            "/__latitude/ui/projects/{project}/archive",
            axum::routing::patch(public_ui_archive_project),
        )
        .route(
            PUBLIC_API_ROOT_TERMINAL_PATH,
            get(public_api_get_root_terminal).post(public_api_post_root_terminal),
        )
        .route(
            PUBLIC_API_ROOT_DESKTOP_PATH,
            get(public_api_get_root_desktop).patch(public_api_patch_root_desktop),
        )
        .route(
            PUBLIC_API_ROOT_TERMINAL_SESSIONS_PATH,
            get(public_api_list_root_terminal_sessions)
                .post(public_api_create_root_terminal_session),
        )
        .route(
            PUBLIC_API_ROOT_TERMINAL_SESSION_PATH,
            delete(public_api_delete_root_terminal_session),
        )
        .route(PUBLIC_API_PROJECT_PATH, get(public_api_get_project))
        .route(
            "/__latitude/api/projects/{project}/archive",
            axum::routing::patch(public_api_patch_project_archive),
        )
        .route(
            PUBLIC_API_PROJECT_DIFF_PATH,
            get(public_api_get_project_diff).patch(public_api_patch_project_diff),
        )
        .route(
            PUBLIC_API_PROJECT_GIT_HISTORY_PATH,
            get(public_api_get_project_git_history),
        )
        .route(
            PUBLIC_API_PROJECT_GIT_COMMIT_PATH,
            get(public_api_get_project_git_commit),
        )
        .route(
            PUBLIC_API_PROJECT_FILES_PATH,
            get(public_api_get_project_files)
                .post(public_api_highlight_project_file)
                .put(public_api_put_project_file),
        )
        .route(
            "/__latitude/ui/files/{project}",
            axum::routing::put(public_ui_put_project_file),
        )
        .route(
            PUBLIC_API_PROJECT_TERMINAL_PATH,
            get(public_api_get_project_terminal).post(public_api_post_project_terminal),
        )
        .route(
            PUBLIC_API_PROJECT_TERMINAL_SESSIONS_PATH,
            get(public_api_list_terminal_sessions).post(public_api_create_terminal_session),
        )
        .route(
            PUBLIC_API_PROJECT_TERMINAL_SESSION_PATH,
            delete(public_api_delete_terminal_session),
        )
        .route(PUBLIC_ROOT_TERMINAL_WS_PATH, get(public_root_terminal_ws))
        .route(PUBLIC_ROOT_DESKTOP_WS_PATH, get(public_root_desktop_ws))
        .route(PUBLIC_TERMINAL_WS_PATH, get(public_terminal_ws))
        .fallback(public_entry)
        .with_state(state)
}

fn command_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/config", get(get_config).put(put_config))
        .route("/projects", get(list_projects).post(create_project))
        .route("/t3code/embed-session", post(create_t3code_embed_session))
        .route(
            "/projects/{project}",
            get(get_project).put(replace_project).delete(delete_project),
        )
        .route(
            "/projects/{project}/deployments",
            get(list_project_deployments).post(create_project_deployment),
        )
        .route(
            "/projects/{project}/deployments/{name}",
            get(get_project_deployment)
                .put(replace_project_deployment)
                .delete(delete_project_deployment),
        )
        .route(
            "/projects/{project}/pages/{name}",
            post(upsert_project_page).put(upsert_project_page),
        )
        .route(
            "/projects/{project}/pages/{name}/content",
            get(get_project_page_content),
        )
        .route(
            "/shares",
            get(list_deployment_shares).post(create_deployment_share),
        )
        .route(
            "/shares/{token}",
            get(get_deployment_share).delete(delete_deployment_share),
        );

    Router::new()
        .route("/health", get(command_health))
        .nest("/api", api)
        .with_state(state)
}
