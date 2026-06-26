use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{
    desktop::{DesktopResolutionError, desktop_info_response, set_desktop_resolution},
    state::AppState,
};

use super::{
    auth::{public_headers_are_authenticated, public_request_is_authenticated},
    constants::{MAX_DESKTOP_ACTION_PAYLOAD_BYTES, PUBLIC_ROOT_DESKTOP_WS_PATH},
    response::json_error,
};

#[derive(Debug, Deserialize)]
pub(super) struct DesktopWsQuery {
    pub(super) token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DesktopActionPayload {
    action: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    screen_id: Option<String>,
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

pub(in crate::server) async fn public_api_patch_root_desktop(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return super::auth::public_api_auth_challenge();
    }

    execute_desktop_action_request(req).await
}

pub(in crate::server) async fn execute_desktop_action_request(
    req: Request<Body>,
) -> Response<Body> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_DESKTOP_ACTION_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("desktop action payload could not be read: {error}"),
            );
        }
    };

    let action = match parse_desktop_action_payload(content_type.as_deref(), &body) {
        Ok(action) => action,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
    };

    match set_desktop_resolution(action.screen_id.as_deref(), action.width, action.height) {
        Ok(response) => Json(response).into_response(),
        Err(DesktopResolutionError::UnsupportedPlatform) => json_error(
            StatusCode::NOT_IMPLEMENTED,
            "desktop resolution changes are only supported on Windows",
        ),
        Err(DesktopResolutionError::InvalidDimensions) => json_error(
            StatusCode::BAD_REQUEST,
            "desktop resolution must be between 640x480 and 7680x4320",
        ),
        Err(DesktopResolutionError::InvalidScreenId(screen_id)) => json_error(
            StatusCode::BAD_REQUEST,
            format!("desktop screen '{screen_id}' is not available for resolution changes"),
        ),
        Err(DesktopResolutionError::CurrentSettingsUnavailable) => json_error(
            StatusCode::BAD_GATEWAY,
            "current display settings could not be read",
        ),
        Err(DesktopResolutionError::ChangeFailed {
            width,
            height,
            code,
        }) => json_error(
            StatusCode::BAD_REQUEST,
            format!("Windows rejected resolution {width}x{height} with display code {code}"),
        ),
    }
}

fn parse_desktop_action_payload(
    content_type: Option<&str>,
    body: &[u8],
) -> Result<DesktopSetResolutionAction, String> {
    let is_json = content_type
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or(value)
                .trim()
                .eq_ignore_ascii_case("application/json")
        })
        .unwrap_or(true);
    if !is_json {
        return Err("desktop actions must use application/json".to_string());
    }

    let payload: DesktopActionPayload = serde_json::from_slice(body)
        .map_err(|error| format!("desktop action JSON is invalid: {error}"))?;
    let action = payload.action.as_deref().unwrap_or("set_resolution");
    if action != "set_resolution" {
        return Err(format!("unsupported desktop action '{action}'"));
    }

    let width = payload
        .width
        .ok_or_else(|| "desktop resolution width is required".to_string())?;
    let height = payload
        .height
        .ok_or_else(|| "desktop resolution height is required".to_string())?;

    Ok(DesktopSetResolutionAction {
        width,
        height,
        screen_id: payload.screen_id,
    })
}

struct DesktopSetResolutionAction {
    width: u32,
    height: u32,
    screen_id: Option<String>,
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
