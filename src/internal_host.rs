use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub(crate) async fn require_bearer_auth(
    State(expected): State<Arc<str>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let authenticated = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|provided| provided == expected.as_ref());

    if authenticated {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
