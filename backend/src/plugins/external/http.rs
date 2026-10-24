//! The routes of an external plugin: everything under `/api/plugins/<name>/<subname>/` goes to the process as an
//! `http_request`, and its answer is the response. The plugin sees the request as data (method, path, query, headers, body
//! in base64), so it needs no web server of its own.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::header::{AUTHORIZATION, CONNECTION, CONTENT_LENGTH, COOKIE, TRANSFER_ENCODING};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;
use serde_json::{Value, json};

use super::process::PluginProcess;
use super::rpc::CallError;
use crate::plugins::base::PluginCaller;

/// A larger request body is refused: it travels inside one protocol line as base64, which has to stay under the line limit
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct Answer {
    status: u16,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body_b64: String,
}

pub fn router(label: String, process: Arc<PluginProcess>) -> Router {
    Router::new().fallback(move |request: Request| {
        let (label, process) = (label.clone(), process.clone());
        async move { forward(&label, &process, request).await }
    })
}

fn failure(status: StatusCode, label: &str, why: impl std::fmt::Display) -> Response {
    (status, format!("the plugin {label} {why}")).into_response()
}

async fn forward(label: &str, process: &PluginProcess, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let Ok(body) = to_bytes(body, MAX_BODY_BYTES).await else {
        return failure(StatusCode::PAYLOAD_TOO_LARGE, label, format!("takes a body of at most {} MiB", MAX_BODY_BYTES / (1024 * 1024)));
    };
    // The caller's own credentials are the backend's business: the plugin is told who is calling, not given the means to be them
    let headers: Vec<[String; 2]> = parts
        .headers
        .iter()
        .filter(|(name, _)| *name != AUTHORIZATION && *name != COOKIE)
        .filter_map(|(name, value)| Some([name.as_str().to_string(), value.to_str().ok()?.to_string()]))
        .collect();
    let params = json!({
        "user_id": parts.extensions.get::<PluginCaller>().map(|caller| caller.0),
        "method": parts.method.as_str(),
        "path": parts.uri.path(),
        "query": parts.uri.query().unwrap_or(""),
        "headers": headers,
        "body_b64": STANDARD.encode(&body),
    });
    match process.call("http_request", params, HTTP_TIMEOUT).await {
        Ok(answer) => respond(label, answer),
        Err(CallError::Rpc(e)) if e.code == super::rpc::METHOD_NOT_FOUND => failure(StatusCode::NOT_FOUND, label, "has no routes"),
        Err(CallError::Rpc(e)) => failure(StatusCode::BAD_GATEWAY, label, format!("failed: {}", e.message)),
        Err(CallError::Timeout) => failure(StatusCode::GATEWAY_TIMEOUT, label, format!("did not answer within {} s", HTTP_TIMEOUT.as_secs())),
        Err(CallError::Closed) => failure(StatusCode::BAD_GATEWAY, label, "is not running"),
    }
}

fn respond(label: &str, answer: Value) -> Response {
    let bad = |why: String| failure(StatusCode::BAD_GATEWAY, label, why);
    let answer: Answer = match serde_json::from_value(answer) {
        Ok(answer) => answer,
        Err(e) => return bad(format!("answered something unusable: {e}")),
    };
    let Ok(status) = StatusCode::from_u16(answer.status) else {
        return bad(format!("answered with the status {}", answer.status));
    };
    let Ok(body) = STANDARD.decode(&answer.body_b64) else {
        return bad("sent a body that is not base64".to_string());
    };
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    for (name, value) in answer.headers {
        let (Ok(name), Ok(value)) = (HeaderName::try_from(name.as_str()), HeaderValue::from_str(&value)) else {
            return bad(format!("sent a header that is not valid: {name}"));
        };
        // The framing is the server's: a plugin that sets these would only contradict the body it sent
        if name == CONTENT_LENGTH || name == TRANSFER_ENCODING || name == CONNECTION {
            continue;
        }
        response.headers_mut().append(name, value);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_answer_becomes_the_response() {
        let response = respond("a/b", json!({"status": 201, "headers": [["content-type", "text/plain"], ["content-length", "99"]], "body_b64": STANDARD.encode("hi")}));
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.headers()["content-type"], "text/plain");
        assert!(response.headers().get("content-length").is_none());
        assert_eq!(&to_bytes(response.into_body(), 100).await.unwrap()[..], b"hi");
    }

    #[test]
    fn an_unusable_answer_is_a_bad_gateway_naming_the_plugin() {
        for answer in [json!({"status": 99999}), json!({"status": 200, "body_b64": "***"}), json!("text"), json!({"status": 200, "headers": [["bad name", "x"]]})] {
            assert_eq!(respond("a/b", answer).status(), StatusCode::BAD_GATEWAY);
        }
    }
}
