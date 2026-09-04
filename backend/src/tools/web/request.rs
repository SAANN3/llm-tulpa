use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, ScopeGrant, Tool, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};

use super::parse_host;

/// How much of a response body gets returned inline when the call doesn't ask for a
/// smaller cap itself and the response isn't HTML (see `HTML_DEFAULT_MAX_RESPONSE_BYTES`)
/// — small enough that a typical JSON API response comes back whole, large enough to be
/// useless-in-practice on anything meant to be downloaded rather than read.
const DEFAULT_MAX_RESPONSE_BYTES: usize = 50_000;

/// The default cap for anything served as `text/html` specifically — a real page's raw
/// markup is mostly tag/script/style noise around a small amount of actual content, so
/// even a *smaller* HTML response wastes far more of a turn's context per byte of
/// signal than the same size of JSON or plain text does. Found the hard way: a single
/// 50KB CAPTCHA page (all markup, no substance) dominated a chat's history and derailed
/// the model's next several turns. Explicitly passing `max_response_bytes` always wins
/// over this — this only changes what happens when the model didn't ask for a specific
/// size, on the assumption it wasn't expecting mostly-markup back in the first place.
const HTML_DEFAULT_MAX_RESPONSE_BYTES: usize = 4_000;

/// Hard ceiling on `max_response_bytes` regardless of what the model asks for — this
/// tool reads the whole body into memory and returns it inline in the reply, unlike
/// `web.download_file`, which streams straight to disk.
const HARD_MAX_RESPONSE_BYTES: usize = 500_000;

pub struct WebRequestTool;

#[derive(Deserialize, tool_derive::ToolParams)]
struct WebRequestArgs {
    #[tool(description = "The HTTP method: 'GET', 'HEAD', 'POST', 'PUT', 'PATCH', or 'DELETE'.")]
    method: String,
    #[tool(description = "The full URL to request (e.g. 'https://api.example.com/v1/thing'). Must include the scheme (http:// or https://).")]
    url: String,
    #[tool(description = "Request headers as a JSON object of name/value string pairs, e.g. {\"Authorization\": \"Bearer ...\", \"Content-Type\": \"application/json\"}. Omit if none are needed.")]
    headers: Option<HashMap<String, String>>,
    #[tool(description = "Raw request body to send (e.g. a JSON string for a POST/PUT). Omit for methods that don't send a body.")]
    body: Option<String>,
    #[tool(description = "Maximum bytes of the response body to return inline. Defaults to 50000 (or just 4000 for an HTML response specifically — raw markup is mostly noise, so ask for more explicitly if you actually need it), hard-capped at 500000 either way. For anything larger, or for binary content, use web.download_file to save it to disk instead.")]
    max_response_bytes: Option<u32>,
}

#[derive(Serialize)]
struct WebRequestOut {
    status_code: u16,
    content_type: String,
    /// Decoded lossily as UTF-8 — a binary response will come back as garbled text
    /// rather than an error. Use `web.download_file` for anything not meant to be
    /// read as text.
    body: String,
    bytes_returned: u64,
    truncated: bool,
}

/// The two permission levels a host can be granted, independent of which specific
/// method within a level is being called — approving any one write-category method
/// (POST/PUT/PATCH/DELETE) for a host is what `is_dangerous` treats as covering
/// every other write-category method for that same host too, not just the one that
/// was actually approved.
enum MethodCategory {
    Read,
    Write,
}

/// `None` for anything not in the tool's own recognized method list — `is_dangerous`
/// treats that as "can't judge this, deny with no escalation" rather than guessing a
/// category for it.
fn method_category(method: &str) -> Option<MethodCategory> {
    match method.to_uppercase().as_str() {
        "GET" | "HEAD" => Some(MethodCategory::Read),
        "POST" | "PUT" | "PATCH" | "DELETE" => Some(MethodCategory::Write),
        _ => None,
    }
}

#[async_trait]
impl Tool for WebRequestTool {
    fn function_name(&self) -> &str {
        "web.request"
    }

    fn description(&self) -> &str {
        "Makes an HTTP request (GET, HEAD, POST, PUT, PATCH, or DELETE) and returns the status \
         code, content type, and response body inline — unlike web.download_file, nothing is \
         written to disk, so this is for reading an API response or a small page right now, not \
         for saving arbitrary or binary content. The body is decoded as UTF-8 (lossily) and \
         capped in size (see max_response_bytes — HTML responses default to a much smaller cap \
         than anything else, since raw markup is mostly noise); a binary response will look \
         garbled — use web.download_file for that instead. Permission is granted per host, not \
         per call: approving GET/HEAD to a host covers GET/HEAD there from then on; approving \
         any one of POST/PUT/PATCH/DELETE to a host covers all four there from then on, and \
         GET/HEAD too."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        WebRequestArgs::tool_properties()
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: WebRequestArgs = serde_json::from_value(data)?;

        let host = match parse_host(&args.url) {
            Some(host) => host,
            None => {
                return Ok(ToolPermission::Denied {
                    reason: format!("couldn't parse a host out of '{}'", args.url),
                    escalation: None,
                });
            }
        };

        let Some(category) = method_category(&args.method) else {
            return Ok(ToolPermission::Denied {
                reason: format!("unrecognized HTTP method '{}'", args.method),
                escalation: None,
            });
        };

        let own = scope.own.as_ref();
        let host_in = |key: &str| {
            own.and_then(|s| s.get(key))
                .and_then(|h| h.as_object())
                .is_some_and(|h| h.contains_key(&host))
        };
        let read_granted = host_in("hosts_read");
        let write_granted = host_in("hosts_write");

        // Write implies read (checked here, not by also writing "hosts_read" whenever
        // "hosts_write" is granted) — one flag per host per level is the only state
        // that ever needs to exist; this is just where the implication is applied.
        let allowed = match category {
            MethodCategory::Read => read_granted || write_granted,
            MethodCategory::Write => write_granted,
        };
        if allowed {
            return Ok(ToolPermission::Allowed);
        }

        let (key, ui_message) = match category {
            MethodCategory::Read => (
                "hosts_read",
                format!("Allow GET/HEAD requests to '{host}'?"),
            ),
            MethodCategory::Write => (
                "hosts_write",
                format!(
                    "Allow POST/PUT/PATCH/DELETE requests to '{host}'? (Approving any one of \
                     these also covers the rest, and GET/HEAD, for '{host}' — not just this \
                     specific call.)"
                ),
            ),
        };

        Ok(ToolPermission::Denied {
            reason: format!("no permission granted for host '{host}'"),
            escalation: Some(ScopeGrant {
                scope: ResolvedScope {
                    own: Some(serde_json::json!({ key: { host.clone(): true } })),
                    shared: HashMap::new(),
                },
                ui_message,
            }),
        })
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: WebRequestArgs = serde_json::from_value(data)?;

        let method = reqwest::Method::from_bytes(args.method.to_uppercase().as_bytes())
            .map_err(|_| ToolError::FailedUnknown(format!("unrecognized HTTP method '{}'", args.method)))?;

        let mut request = reqwest::Client::new().request(method, &args.url);

        if let Some(headers) = &args.headers {
            let mut header_map = reqwest::header::HeaderMap::new();
            for (name, value) in headers {
                let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|e| ToolError::FailedUnknown(format!("invalid header name '{name}': {e}")))?;
                let header_value = reqwest::header::HeaderValue::from_str(value)
                    .map_err(|e| ToolError::FailedUnknown(format!("invalid header value for '{name}': {e}")))?;
                header_map.insert(header_name, header_value);
            }
            request = request.headers(header_map);
        }

        if let Some(body) = args.body.clone() {
            request = request.body(body);
        }

        let response = request
            .send()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("request to {} failed: {e}", args.url)))?;

        let status_code = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let is_html = content_type.to_ascii_lowercase().starts_with("text/html");
        let default_cap = if is_html { HTML_DEFAULT_MAX_RESPONSE_BYTES } else { DEFAULT_MAX_RESPONSE_BYTES };
        let cap = (args.max_response_bytes.map(|b| b as usize).unwrap_or(default_cap)).min(HARD_MAX_RESPONSE_BYTES);

        let bytes = response
            .bytes()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read response body: {e}")))?;

        let truncated = bytes.len() > cap;
        let returned = &bytes[..bytes.len().min(cap)];
        let body = String::from_utf8_lossy(returned).to_string();

        Ok(serde_json::to_value(WebRequestOut {
            status_code,
            content_type,
            body,
            bytes_returned: returned.len() as u64,
            truncated,
        })?)
    }
}
