use std::{io, path::Path};

use axum::{
    body::Body,
    http::{Method, Response, StatusCode, header},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
};
use tokio_util::io::ReaderStream;

pub(crate) fn streaming_request_body(body: Body) -> reqwest::Body {
    reqwest::Body::wrap_stream(body.into_data_stream())
}

pub(crate) fn streaming_http_response(upstream: reqwest::Response) -> Response<Body> {
    let status = upstream.status();
    let mut response = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        if !is_hop_by_hop_header(name.as_str()) {
            response = response.header(name, value);
        }
    }
    response
        .body(Body::from_stream(upstream.bytes_stream()))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("internal server error\n"))
                .expect("static response should be valid")
        })
}

pub(crate) fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ByteRange {
    start: u64,
    end: u64,
}

pub(crate) async fn file_response(
    method: &Method,
    range_header: Option<&str>,
    path: &Path,
    media_type: &str,
) -> io::Result<Response<Body>> {
    let metadata = fs::metadata(path).await?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "path is not a file",
        ));
    }

    let length = metadata.len();
    let requested_range = range_header.and_then(|value| value.strip_prefix("bytes="));
    let byte_range = match requested_range {
        Some(value) => match parse_byte_range(value, length) {
            Some(range) => Some(range),
            None => return range_not_satisfiable(length),
        },
        None => None,
    };
    let (status, start, response_length) = match byte_range {
        Some(range) => (
            StatusCode::PARTIAL_CONTENT,
            range.start,
            range.end - range.start + 1,
        ),
        None => (StatusCode::OK, 0, length),
    };

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, media_type)
        .header(header::CONTENT_LENGTH, response_length)
        .header(header::ACCEPT_RANGES, "bytes");
    if let Some(range) = byte_range {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", range.start, range.end, length),
        );
    }

    if method == Method::HEAD {
        return builder.body(Body::empty()).map_err(io::Error::other);
    }

    let mut file = fs::File::open(path).await?;
    if start != 0 {
        file.seek(SeekFrom::Start(start)).await?;
    }
    let stream = ReaderStream::new(file.take(response_length));
    builder
        .body(Body::from_stream(stream))
        .map_err(io::Error::other)
}

fn range_not_satisfiable(length: u64) -> io::Result<Response<Body>> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{length}"))
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::empty())
        .map_err(io::Error::other)
}

fn parse_byte_range(value: &str, length: u64) -> Option<ByteRange> {
    if length == 0 || value.contains(',') {
        return None;
    }
    let (start, end) = value.trim().split_once('-')?;
    if start.is_empty() {
        let suffix_length = end.parse::<u64>().ok()?;
        if suffix_length == 0 {
            return None;
        }
        let start = length.saturating_sub(suffix_length);
        return Some(ByteRange {
            start,
            end: length - 1,
        });
    }

    let start = start.parse::<u64>().ok()?;
    if start >= length {
        return None;
    }
    let end = if end.is_empty() {
        length - 1
    } else {
        end.parse::<u64>().ok()?.min(length - 1)
    };
    (end >= start).then_some(ByteRange { start, end })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[test]
    fn parses_supported_single_byte_ranges() {
        assert_eq!(
            parse_byte_range("2-5", 10),
            Some(ByteRange { start: 2, end: 5 })
        );
        assert_eq!(
            parse_byte_range("7-", 10),
            Some(ByteRange { start: 7, end: 9 })
        );
        assert_eq!(
            parse_byte_range("-3", 10),
            Some(ByteRange { start: 7, end: 9 })
        );
        assert_eq!(
            parse_byte_range("-20", 10),
            Some(ByteRange { start: 0, end: 9 })
        );
    }

    #[test]
    fn rejects_unsatisfiable_or_multiple_ranges() {
        assert_eq!(parse_byte_range("10-", 10), None);
        assert_eq!(parse_byte_range("5-2", 10), None);
        assert_eq!(parse_byte_range("0-1,4-5", 10), None);
        assert_eq!(parse_byte_range("-0", 10), None);
        assert_eq!(parse_byte_range("0-", 0), None);
    }

    #[tokio::test]
    async fn streams_only_the_requested_file_range() {
        let path = std::env::temp_dir().join(format!(
            "latitude-range-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"0123456789").await.unwrap();

        let response = file_response(&Method::GET, Some("bytes=3-6"), &path, "text/plain")
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 3-6/10"
        );
        let body = to_bytes(response.into_body(), 4).await.unwrap();
        assert_eq!(body.as_ref(), b"3456");

        fs::remove_file(path).await.unwrap();
    }
}
