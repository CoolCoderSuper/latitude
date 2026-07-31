use axum::{
    Json,
    body::Body,
    http::{Method, Response, StatusCode, header},
    response::IntoResponse,
};
use serde::Serialize;
use tracing::error;

use crate::{config::ConfigError, project_files::ProjectFileError, storage::StorageError};

#[derive(Debug)]
pub(super) struct ApiError {
    pub(super) status: StatusCode,
    pub(super) message: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ErrorBody {
    pub(super) error: String,
}

impl ApiError {
    pub(super) fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(super) fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response<Body> {
        json_error(self.status, self.message)
    }
}

impl From<ConfigError> for ApiError {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::Invalid(message) => Self::new(StatusCode::BAD_REQUEST, message),
            error => {
                error!(%error, "config operation failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
        }
    }
}

impl From<StorageError> for ApiError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Invalid(message) => Self::new(StatusCode::BAD_REQUEST, message),
            error => {
                error!(%error, "catalog operation failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
        }
    }
}

impl From<ProjectFileError> for ApiError {
    fn from(error: ProjectFileError) -> Self {
        Self::new(error.status, error.message)
    }
}

impl From<(StatusCode, String)> for ApiError {
    fn from((status, message): (StatusCode, String)) -> Self {
        Self::new(status, message)
    }
}

pub(super) fn json_error(status: StatusCode, message: impl Into<String>) -> Response<Body> {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}

pub(super) fn plain_response(status: StatusCode, body: impl Into<Body>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body.into())
        .unwrap_or_else(internal_response)
}

pub(super) fn html_response(method: &Method, html: String) -> Response<Body> {
    html_status_response(StatusCode::OK, method, html)
}

pub(super) fn html_status_response(
    status: StatusCode,
    method: &Method,
    html: String,
) -> Response<Body> {
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if method == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

pub(super) fn internal_response(_: axum::http::Error) -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from("internal server error\n"))
        .expect("static response should be valid")
}
