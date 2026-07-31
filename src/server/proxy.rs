use std::time::Duration;

use axum::{
    body::Body,
    http::{Request, Response, header},
};

use crate::http_stream::{is_hop_by_hop_header, streaming_http_response, streaming_request_body};

use super::paths::filtered_cookie_header;

pub(super) enum HttpProxyError {
    Request(reqwest::Error),
    Timeout,
}

pub(super) async fn proxy_http(
    client: &reqwest::Client,
    req: Request<Body>,
    target_url: String,
    excluded_cookie_names: &[&str],
) -> Result<Response<Body>, HttpProxyError> {
    let (parts, body) = req.into_parts();
    let mut request = client.request(parts.method, target_url);
    for (name, value) in &parts.headers {
        if is_hop_by_hop_header(name.as_str()) || *name == header::HOST {
            continue;
        }
        if *name == header::COOKIE {
            if let Some(value) = filtered_cookie_header(value, excluded_cookie_names) {
                request = request.header(name, value);
            }
            continue;
        }
        request = request.header(name, value);
    }

    match tokio::time::timeout(
        Duration::from_secs(60),
        request.body(streaming_request_body(body)).send(),
    )
    .await
    {
        Ok(Ok(response)) => Ok(streaming_http_response(response)),
        Ok(Err(error)) => Err(HttpProxyError::Request(error)),
        Err(_) => Err(HttpProxyError::Timeout),
    }
}
