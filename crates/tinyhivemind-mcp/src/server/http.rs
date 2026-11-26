//! Admission of one HTTP request, before any JSON-RPC is read.
//!
//! In order: exactly one `Host` must name this listener (`127.0.0.1:<port>` or
//! `localhost:<port>`), any `Origin` is refused, the path must be `/mcp`, the
//! bearer must be a served seat's, the method must be POST (a GET for the
//! optional event stream gets 405, as the Streamable HTTP transport allows),
//! an `MCP-Protocol-Version` header must name a served revision, and the
//! body must fit [`super::ServerOptions::max_body_bytes`] and arrive within
//! its timeout. Nothing here ever answers with a redirect.
use super::rpc::{self, Answer};
use super::{PROTOCOL_VERSIONS, Shared};
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{self, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use std::convert::Infallible;
use std::sync::Arc;

/// Answer `request`.
///
/// # Errors
/// Never; every failure is an HTTP status.
pub(super) async fn handle(
    shared: Arc<Shared>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    Ok(admit(&shared, request).await)
}

async fn admit(shared: &Arc<Shared>, request: Request<Incoming>) -> Response<Full<Bytes>> {
    let headers = request.headers();
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts.next().and_then(|value| value.to_str().ok());
    let port = shared.port;
    let own = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let named = host.is_some_and(|host| own.iter().any(|own| own == host));
    if !named || hosts.next().is_some() {
        return status(StatusCode::FORBIDDEN);
    }
    if headers.contains_key(header::ORIGIN) {
        return status(StatusCode::FORBIDDEN);
    }
    if request.uri().path() != "/mcp" {
        return status(StatusCode::NOT_FOUND);
    }
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.as_bytes().strip_prefix(b"Bearer "));
    let Some(seat) = presented.and_then(|bearer| shared.seat(bearer)) else {
        let mut refused = status(StatusCode::UNAUTHORIZED);
        refused
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return refused;
    };
    if request.method() != Method::POST {
        let mut refused = status(StatusCode::METHOD_NOT_ALLOWED);
        refused
            .headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("POST"));
        return refused;
    }
    let version = headers
        .get("mcp-protocol-version")
        .map(HeaderValue::as_bytes);
    if version.is_some_and(|version| !PROTOCOL_VERSIONS.iter().any(|v| v.as_bytes() == version)) {
        return status(StatusCode::BAD_REQUEST);
    }
    let limit = shared.options.max_body_bytes;
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if declared.is_some_and(|length| length > limit) {
        return status(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let body = Limited::new(request.into_body(), limit).collect();
    let bytes = match tokio::time::timeout(shared.options.body_timeout, body).await {
        Err(_) => return status(StatusCode::REQUEST_TIMEOUT),
        Ok(Err(error)) if error.downcast_ref::<LengthLimitError>().is_some() => {
            return status(StatusCode::PAYLOAD_TOO_LARGE);
        }
        Ok(Err(_)) => return status(StatusCode::BAD_REQUEST),
        Ok(Ok(collected)) => collected.to_bytes(),
    };
    match rpc::answer(shared, seat, &bytes).await {
        Answer::Accepted => status(StatusCode::ACCEPTED),
        Answer::Reply(code, message) => {
            let mut response = Response::new(Full::new(Bytes::from(message.to_string())));
            *response.status_mut() = code;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response
        }
    }
}

/// An empty response with `code`.
fn status(code: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = code;
    response
}
