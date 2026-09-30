use super::{auth::header_cookie_value, public::enabled_project, response::ApiError};
use crate::state::AppState;
use axum::{
    extract::{Path, RawQuery, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Redirect, Response},
};

pub(super) async fn open(
    Path(project): Path<String>,
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    enabled_project(&state, &project).await?;
    let editor = if header_cookie_value(&headers, "latitude_editor").as_deref() == Some("neovim") {
        "_neovim"
    } else {
        "_files"
    };
    let mut location = format!("/{project}/{editor}");
    if let Some(query) = query {
        location.push('?');
        location.push_str(&query);
    }
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Redirect::temporary(&location),
    )
        .into_response())
}
