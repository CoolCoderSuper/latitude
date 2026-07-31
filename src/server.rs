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

use std::net::SocketAddr;

use axum::{
    Router, middleware,
    routing::{any, delete, get, post},
};
use tokio::net::TcpListener;
use tracing::info;

use crate::{
    command_protocol::{
        CONFIG_PATH as COMMAND_CONFIG_PATH, HEALTH_PATH as COMMAND_HEALTH_PATH,
        PROJECT_DEPLOYMENT_PATH as COMMAND_PROJECT_DEPLOYMENT_PATH,
        PROJECT_DEPLOYMENTS_PATH as COMMAND_PROJECT_DEPLOYMENTS_PATH,
        PROJECT_PAGE_CONTENT_PATH as COMMAND_PROJECT_PAGE_CONTENT_PATH,
        PROJECT_PAGE_PATH as COMMAND_PROJECT_PAGE_PATH, PROJECT_PATH as COMMAND_PROJECT_PATH,
        PROJECTS_PATH as COMMAND_PROJECTS_PATH, SHARE_PATH as COMMAND_SHARE_PATH,
        SHARES_PATH as COMMAND_SHARES_PATH,
        T3CODE_EMBED_SESSION_PATH as COMMAND_T3CODE_EMBED_SESSION_PATH,
    },
    state::AppState,
};

pub(crate) use git::GitStatusSummary;
pub(crate) use git::file_baseline;
pub(crate) use terminal_api::terminal_websocket_session;

use assets::{ASSET_BASE_PATH, public_asset};
use auth::{open_t3code_embed, require_public_api_auth, require_public_auth};
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
    PUBLIC_ROOT_DESKTOP_WS_PATH, PUBLIC_ROOT_TERMINAL_WS_PATH, PUBLIC_SHARE_BASE_PATH,
    PUBLIC_TERMINAL_WS_PATH, T3CODE_EMBED_PATH,
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
    public_api_session, public_deployment, public_home, public_not_found, public_project_diff,
    public_project_files, public_project_home, public_project_terminal, public_root_desktop,
    public_root_terminal, public_root_terminal_ws, public_share, public_share_not_found,
    public_terminal_ws, public_ui_archive_project, public_ui_create_share, public_ui_delete_share,
    public_ui_get_shares,
};
use t3code::{open_project_in_t3code, open_t3code, t3code_gateway_router};

pub(crate) async fn run(state: AppState) -> anyhow::Result<()> {
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
            result = axum::serve(public_listener, public_router.into_make_service_with_connect_info::<SocketAddr>()) => { result?; }
            result = axum::serve(command_listener, command_router) => { result?; }
            result = axum::serve(gateway_listener, gateway_router) => { result?; }
        }
    } else {
        tokio::select! {
            result = axum::serve(public_listener, public_router.into_make_service_with_connect_info::<SocketAddr>()) => { result?; }
            result = axum::serve(command_listener, command_router) => { result?; }
        }
    }

    Ok(())
}

fn public_router(state: AppState) -> Router {
    Router::new()
        .route(&format!("{ASSET_BASE_PATH}/{{name}}"), get(public_asset))
        .route(LOGIN_PATH, get(get_public_login).post(post_public_login))
        .route(T3CODE_EMBED_PATH, get(open_t3code_embed))
        .route(
            PUBLIC_API_SESSION_PATH,
            get(public_api_session).post(public_api_login),
        )
        .route(PUBLIC_ROOT_TERMINAL_WS_PATH, get(public_root_terminal_ws))
        .route(PUBLIC_ROOT_DESKTOP_WS_PATH, get(public_root_desktop_ws))
        .route(PUBLIC_TERMINAL_WS_PATH, get(public_terminal_ws))
        .route(PUBLIC_SHARE_BASE_PATH, any(public_share_not_found))
        .route(
            &format!("{PUBLIC_SHARE_BASE_PATH}/{{token}}"),
            any(public_share),
        )
        .route(
            &format!("{PUBLIC_SHARE_BASE_PATH}/{{token}}/"),
            any(public_share),
        )
        .route(
            &format!("{PUBLIC_SHARE_BASE_PATH}/{{token}}/{{*remainder}}"),
            any(public_share),
        )
        .merge(protected_public_router(state.clone()))
        .merge(public_pages_router(state.clone()))
        .with_state(state)
}

fn public_pages_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", any(public_home))
        .route("/_terminal", any(public_root_terminal))
        .route("/_terminal/", any(public_root_terminal))
        .route("/_terminal/{*remainder}", any(public_root_terminal))
        .route("/_desktop", any(public_root_desktop))
        .route("/_desktop/", any(public_root_desktop))
        .route("/_desktop/{*remainder}", any(public_root_desktop))
        .route("/{project}", any(public_project_home))
        .route("/{project}/", any(public_project_home))
        .route("/{project}/_diff", any(public_project_diff))
        .route("/{project}/_diff/", any(public_project_diff))
        .route("/{project}/_diff/{*remainder}", any(public_project_diff))
        .route("/{project}/_files", any(public_project_files))
        .route("/{project}/_files/", any(public_project_files))
        .route("/{project}/_files/{*remainder}", any(public_project_files))
        .route("/{project}/_terminal", any(public_project_terminal))
        .route("/{project}/_terminal/", any(public_project_terminal))
        .route(
            "/{project}/_terminal/{*remainder}",
            any(public_project_terminal),
        )
        .route("/{project}/{deployment}", any(public_deployment))
        .route("/{project}/{deployment}/", any(public_deployment))
        .route(
            "/{project}/{deployment}/{*remainder}",
            any(public_deployment),
        )
        .fallback(public_not_found)
        .route_layer(middleware::from_fn_with_state(state, require_public_auth))
}

fn protected_public_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/__latitude/t3code", get(open_t3code))
        .route("/__latitude/t3code/{project}", get(open_project_in_t3code))
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
        .route_layer(middleware::from_fn_with_state(
            state,
            require_public_api_auth,
        ))
}

fn command_router(state: AppState) -> Router {
    Router::new()
        .route(COMMAND_CONFIG_PATH, get(get_config).put(put_config))
        .route(
            COMMAND_PROJECTS_PATH,
            get(list_projects).post(create_project),
        )
        .route(
            COMMAND_T3CODE_EMBED_SESSION_PATH,
            post(create_t3code_embed_session),
        )
        .route(
            COMMAND_PROJECT_PATH,
            get(get_project).put(replace_project).delete(delete_project),
        )
        .route(
            COMMAND_PROJECT_DEPLOYMENTS_PATH,
            get(list_project_deployments).post(create_project_deployment),
        )
        .route(
            COMMAND_PROJECT_DEPLOYMENT_PATH,
            get(get_project_deployment)
                .put(replace_project_deployment)
                .delete(delete_project_deployment),
        )
        .route(
            COMMAND_PROJECT_PAGE_PATH,
            post(upsert_project_page).put(upsert_project_page),
        )
        .route(
            COMMAND_PROJECT_PAGE_CONTENT_PATH,
            get(get_project_page_content),
        )
        .route(
            COMMAND_SHARES_PATH,
            get(list_deployment_shares).post(create_deployment_share),
        )
        .route(
            COMMAND_SHARE_PATH,
            get(get_deployment_share).delete(delete_deployment_share),
        )
        .route(COMMAND_HEALTH_PATH, get(command_health))
        .with_state(state)
}
