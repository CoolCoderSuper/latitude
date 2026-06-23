mod assets;
mod auth;
mod command;
mod constants;
mod git;
mod page;
mod paths;
mod public;
mod render;
mod response;
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

use command::{
    command_health, create_project, create_project_deployment, delete_project,
    delete_project_deployment, get_config, get_project, get_project_deployment,
    list_project_deployments, list_projects, put_config, replace_project,
    replace_project_deployment, upsert_project_page,
};
use constants::{
    LOGIN_PATH, PUBLIC_API_PROJECT_DIFF_PATH, PUBLIC_API_PROJECT_PATH,
    PUBLIC_API_PROJECT_TERMINAL_PATH, PUBLIC_API_PROJECT_TERMINAL_SESSION_PATH,
    PUBLIC_API_PROJECT_TERMINAL_SESSIONS_PATH, PUBLIC_API_PROJECTS_PATH, PUBLIC_API_SESSION_PATH,
    PUBLIC_TERMINAL_WS_PATH,
};
use public::{
    get_public_login, post_public_login, public_api_create_terminal_session,
    public_api_delete_terminal_session, public_api_get_project, public_api_get_project_diff,
    public_api_get_project_terminal, public_api_list_projects, public_api_list_terminal_sessions,
    public_api_login, public_api_patch_project_diff, public_api_post_project_terminal,
    public_api_session, public_entry, public_terminal_ws,
};

pub async fn run(state: AppState) -> anyhow::Result<()> {
    let config = state.config_snapshot().await;
    let public_bind = config.public_bind.clone();
    let command_bind = config.command_bind.clone();

    let public_listener = TcpListener::bind(&public_bind).await?;
    let command_listener = TcpListener::bind(&command_bind).await?;

    info!(bind = %public_bind, "public proxy listening");
    info!(bind = %command_bind, "command API listening");

    let public_router = public_router(state.clone());
    let command_router = command_router(state);

    tokio::select! {
        result = axum::serve(public_listener, public_router) => {
            result?;
        }
        result = axum::serve(command_listener, command_router) => {
            result?;
        }
    }

    Ok(())
}

fn public_router(state: AppState) -> Router {
    Router::new()
        .route(LOGIN_PATH, get(get_public_login).post(post_public_login))
        .route(
            PUBLIC_API_SESSION_PATH,
            get(public_api_session).post(public_api_login),
        )
        .route(PUBLIC_API_PROJECTS_PATH, get(public_api_list_projects))
        .route(PUBLIC_API_PROJECT_PATH, get(public_api_get_project))
        .route(
            PUBLIC_API_PROJECT_DIFF_PATH,
            get(public_api_get_project_diff).patch(public_api_patch_project_diff),
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
        .route(PUBLIC_TERMINAL_WS_PATH, get(public_terminal_ws))
        .fallback(public_entry)
        .with_state(state)
}

fn command_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/config", get(get_config).put(put_config))
        .route("/projects", get(list_projects).post(create_project))
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
        );

    Router::new()
        .route("/health", get(command_health))
        .nest("/api", api)
        .with_state(state)
}
