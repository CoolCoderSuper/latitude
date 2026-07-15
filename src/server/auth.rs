use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Method, Request, Response, StatusCode, header},
};

use crate::{config::BootConfig, state::AppState};

use super::{
    constants::{
        AUTH_COOKIE_MAX_AGE_SECONDS, AUTH_COOKIE_NAME, LATITUDE_THEME_COOKIE, LOGIN_PATH,
        T3CODE_EMBED_COOKIE,
    },
    render::render_public_login,
    response::{internal_response, json_error},
};

#[derive(Debug, Default)]
pub(super) struct PublicLoginForm {
    pub(super) password: String,
    pub(super) next: Option<String>,
}

pub(super) fn public_request_is_authenticated(
    state: &AppState,
    config: &BootConfig,
    req: &Request<Body>,
) -> bool {
    public_headers_are_authenticated(state, config, req.headers(), None)
}

pub(super) fn public_headers_are_authenticated(
    state: &AppState,
    config: &BootConfig,
    headers: &HeaderMap,
    query_token: Option<&str>,
) -> bool {
    header_cookie_value(headers, AUTH_COOKIE_NAME)
        .as_deref()
        .is_some_and(|value| state.verify_public_auth_cookie(&config.public_password, value))
        || header_bearer_token(headers)
            .as_deref()
            .is_some_and(|value| state.verify_public_auth_cookie(&config.public_password, value))
        || query_token
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some_and(|value| state.verify_public_auth_cookie(&config.public_password, value))
}

pub(super) fn public_api_auth_challenge() -> Response<Body> {
    json_error(StatusCode::UNAUTHORIZED, "authentication required")
}

pub(super) fn public_auth_challenge(
    state: &AppState,
    req: &Request<Body>,
    login_failed: bool,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD || request_wants_json(req) {
        return json_error(StatusCode::UNAUTHORIZED, "authentication required");
    }

    let next = clean_next_path(
        req.uri()
            .path_and_query()
            .map(|path_and_query| path_and_query.as_str().to_string()),
    );
    public_login_response(
        StatusCode::UNAUTHORIZED,
        &next,
        login_failed,
        req.method() == Method::HEAD,
        state.device_hostname(),
    )
}

fn request_wants_json(req: &Request<Body>) -> bool {
    req.headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"))
}

pub(super) fn header_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    for value in headers.get_all(header::COOKIE) {
        let Ok(raw) = value.to_str() else {
            continue;
        };

        for cookie in raw.split(';') {
            let Some((cookie_name, cookie_value)) = cookie.trim().split_once('=') else {
                continue;
            };
            if cookie_name.trim() == name {
                return Some(cookie_value.trim().to_string());
            }
        }
    }

    None
}

pub(super) fn request_bearer_token(req: &Request<Body>) -> Option<String> {
    header_bearer_token(req.headers())
}

fn header_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub(super) fn public_auth_set_cookie(state: &AppState, password: &str) -> String {
    let value = state.public_auth_cookie_value(password);
    format!(
        "{AUTH_COOKIE_NAME}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={AUTH_COOKIE_MAX_AGE_SECONDS}"
    )
}

pub(super) fn t3code_embed_set_cookies(
    state: &AppState,
    password: &str,
    theme: &str,
) -> [String; 3] {
    [
        public_auth_set_cookie(state, password),
        format!(
            "{T3CODE_EMBED_COOKIE}=1; SameSite=Lax; Path=/; Max-Age={AUTH_COOKIE_MAX_AGE_SECONDS}"
        ),
        format!(
            "{LATITUDE_THEME_COOKIE}={theme}; SameSite=Lax; Path=/; Max-Age={AUTH_COOKIE_MAX_AGE_SECONDS}"
        ),
    ]
}

pub(super) fn request_is_t3code_embed(headers: &HeaderMap) -> bool {
    header_cookie_value(headers, T3CODE_EMBED_COOKIE).as_deref() == Some("1")
}

pub(super) async fn open_t3code_embed(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let query = req.uri().query().unwrap_or_default();
    let values = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    let config = state.config_snapshot().await;
    let token = values.get("token").map(String::as_str);
    if !public_headers_are_authenticated(&state, &config, req.headers(), token) {
        return public_api_auth_challenge();
    }
    let theme = match values.get("theme").map(String::as_str) {
        Some("light") => "light",
        Some("dark") => "dark",
        _ => return json_error(StatusCode::BAD_REQUEST, "invalid theme"),
    };
    let next = clean_next_path(values.get("next").cloned());
    let mut response = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, next)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap_or_else(internal_response);
    for cookie in t3code_embed_set_cookies(&state, &config.public_password, theme) {
        if let Ok(value) = cookie.parse() {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

pub(super) fn public_login_success_response(next: &str, set_cookie: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, next)
        .header(header::SET_COOKIE, set_cookie)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap_or_else(internal_response)
}

pub(super) fn public_login_response(
    status: StatusCode,
    next: &str,
    login_failed: bool,
    head: bool,
    device_hostname: &str,
) -> Response<Body> {
    let html = render_public_login(next, login_failed, device_hostname);
    let content_length = html.len();
    let body = if head {
        Body::empty()
    } else {
        Body::from(html)
    };

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, content_length)
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .unwrap_or_else(internal_response)
}

pub(super) fn public_login_next_from_query(query: Option<&str>) -> Option<String> {
    let query = query?;
    url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
        if key == "next" {
            Some(value.into_owned())
        } else {
            None
        }
    })
}

pub(super) fn parse_public_login_form(body: &[u8]) -> PublicLoginForm {
    let mut form = PublicLoginForm::default();
    for (key, value) in url::form_urlencoded::parse(body) {
        match key.as_ref() {
            "password" => form.password = value.into_owned(),
            "next" => form.next = Some(value.into_owned()),
            _ => {}
        }
    }
    form
}

pub(super) fn public_password_matches(submitted: &str, expected: &str) -> bool {
    let submitted = submitted.as_bytes();
    let expected = expected.as_bytes();
    let max_len = submitted.len().max(expected.len());
    let mut diff = submitted.len() ^ expected.len();

    for index in 0..max_len {
        let left = submitted.get(index).copied().unwrap_or(0);
        let right = expected.get(index).copied().unwrap_or(0);
        diff |= usize::from(left ^ right);
    }

    diff == 0
}

pub(super) fn clean_next_path(next: Option<String>) -> String {
    let next = next.unwrap_or_else(|| "/".to_string());
    let next = next.trim();
    if !next.starts_with('/')
        || next.starts_with("//")
        || next.starts_with(LOGIN_PATH)
        || !next.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return "/".to_string();
    }

    next.to_string()
}
