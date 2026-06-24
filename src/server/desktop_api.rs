use axum::{
    Json,
    body::Body,
    extract::{Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Response, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{desktop::desktop_info_response, state::AppState};

use super::{
    auth::{public_headers_are_authenticated, public_request_is_authenticated},
    constants::PUBLIC_ROOT_DESKTOP_WS_PATH,
    response::json_error,
};

#[derive(Debug, Deserialize)]
pub(super) struct DesktopWsQuery {
    pub(super) token: Option<String>,
}

pub(in crate::server) async fn public_api_get_root_desktop(
    State(state): State<AppState>,
    req: axum::http::Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return super::auth::public_api_auth_challenge();
    }

    if !config.desktop.enabled {
        return json_error(StatusCode::NOT_FOUND, "desktop is not enabled");
    }

    let target = match state.desktop_manager().target_for(&config.desktop).await {
        Ok(target) => target,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("desktop target could not be prepared: {error}"),
            );
        }
    };

    Json(desktop_info_response(
        &config.desktop,
        &target,
        PUBLIC_ROOT_DESKTOP_WS_PATH.to_string(),
    ))
    .into_response()
}

pub(in crate::server) async fn public_root_desktop_ws(
    Query(query): Query<DesktopWsQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, query.token.as_deref()) {
        return super::auth::public_api_auth_challenge();
    }

    if !config.desktop.enabled {
        return json_error(StatusCode::NOT_FOUND, "desktop is not enabled");
    }

    let target = match state.desktop_manager().target_for(&config.desktop).await {
        Ok(target) => target,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("desktop target could not be prepared: {error}"),
            );
        }
    };

    ws.on_upgrade(move |socket| crate::desktop::desktop_websocket_session(socket, target))
}
