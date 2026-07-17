//! HTTP range streaming of original media files. The server never transcodes
//! it just serves bytes, honouring the `Range` header so clients can seek.

use std::io::SeekFrom;
use std::path::Path;

use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::json_error;

/// Stream a file, honouring an optional `Range: bytes=start-end` header.
///
/// Returns `206 Partial Content` with a `Content-Range` when a satisfiable
/// range is requested, otherwise a full `200 OK`. The body is streamed and the
/// file is never fully buffered in memory.
pub async fn stream_file(path: &Path, req_headers: &HeaderMap) -> Response {
    let mut file = match File::open(path).await {
        Ok(f) => f,
        Err(_) => {
            return json_error(StatusCode::NOT_FOUND, "media file not found on disk");
        }
    };

    let total_size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not stat media file",
            );
        }
    };

    let content_type = content_type_for(path);

    match parse_range(req_headers, total_size) {
        RangeOutcome::Full => full_response(file, total_size, content_type),
        RangeOutcome::Partial { start, end } => {
            if file.seek(SeekFrom::Start(start)).await.is_err() {
                return json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "could not seek media file",
                );
            }
            partial_response(file, start, end, total_size, content_type)
        }
        RangeOutcome::Unsatisfiable => {
            let mut resp = json_error(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "requested range not satisfiable",
            );
            resp.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{}", total_size))
                    .unwrap_or(HeaderValue::from_static("bytes */0")),
            );
            resp
        }
    }
}

/// Full `200 OK` response streaming the whole file.
fn full_response(file: File, total_size: u64, content_type: &str) -> Response {
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut resp = Response::new(body);
    let headers = resp.headers_mut();
    set_common_headers(headers, content_type);
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(total_size),
    );
    resp
}

/// `206 Partial Content` response streaming `[start, end]` inclusive.
fn partial_response(
    file: File,
    start: u64,
    end: u64,
    total_size: u64,
    content_type: &str,
) -> Response {
    let length = end - start + 1;
    // Limit the reader to exactly the requested window.
    let limited = file.take(length);
    let stream = ReaderStream::new(limited);
    let body = Body::from_stream(stream);

    let mut resp = Response::new(body);
    *resp.status_mut() = StatusCode::PARTIAL_CONTENT;
    let headers = resp.headers_mut();
    set_common_headers(headers, content_type);
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    headers.insert(
        header::CONTENT_RANGE,
        HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, total_size))
            .unwrap_or(HeaderValue::from_static("bytes 0-0/0")),
    );
    resp
}

fn set_common_headers(headers: &mut HeaderMap, content_type: &str) {
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).unwrap_or(HeaderValue::from_static(
            "application/octet-stream",
        )),
    );
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
}

/// Outcome of parsing a `Range` header against a known file size.
enum RangeOutcome {
    Full,
    Partial { start: u64, end: u64 },
    Unsatisfiable,
}

/// Parse a single-range `bytes=` header. Multi-range requests fall back to a
/// full response (the common, simple behaviour for media servers).
fn parse_range(headers: &HeaderMap, total_size: u64) -> RangeOutcome {
    let raw = match headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        Some(r) => r,
        None => return RangeOutcome::Full,
    };

    let spec = match raw.strip_prefix("bytes=") {
        Some(s) => s.trim(),
        None => return RangeOutcome::Full,
    };

    // Reject multi-range; serve the whole file instead of erroring.
    if spec.contains(',') {
        return RangeOutcome::Full;
    }

    if total_size == 0 {
        return RangeOutcome::Unsatisfiable;
    }

    let (start_str, end_str) = match spec.split_once('-') {
        Some(parts) => parts,
        None => return RangeOutcome::Full,
    };

    let last = total_size - 1;

    if start_str.is_empty() {
        // Suffix range: bytes=-N → last N bytes.
        let n: u64 = match end_str.parse() {
            Ok(n) if n > 0 => n,
            _ => return RangeOutcome::Unsatisfiable,
        };
        let start = total_size.saturating_sub(n);
        return RangeOutcome::Partial { start, end: last };
    }

    let start: u64 = match start_str.parse() {
        Ok(s) => s,
        Err(_) => return RangeOutcome::Unsatisfiable,
    };

    if start > last {
        return RangeOutcome::Unsatisfiable;
    }

    let end: u64 = if end_str.is_empty() {
        last
    } else {
        match end_str.parse::<u64>() {
            Ok(e) => e.min(last),
            Err(_) => return RangeOutcome::Unsatisfiable,
        }
    };

    if end < start {
        return RangeOutcome::Unsatisfiable;
    }

    RangeOutcome::Partial { start, end }
}

/// Pick a Content-Type from the container extension, with media-friendly
/// defaults the generic mime table doesn't always get right.
fn content_type_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "ts" => "video/mp2t",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

/// Convenience: build a stream response or a JSON 404 for demo items.
pub async fn stream_or_demo_error(abs_path: Option<&str>, req_headers: &HeaderMap) -> Response {
    match abs_path {
        Some(p) => stream_file(Path::new(p), req_headers).await,
        None => json_error(StatusCode::NOT_FOUND, "demo item has no media").into_response(),
    }
}
