//! The plugin's own routes: a request to `/api/plugins/<name>/<subname>/...` arrives as an `HttpRequest`, and the answer is
//! the response the caller gets.

use serde_json::Value;

/// A request to one of the plugin's routes. Only the owner reaches them; `user_id` says who. The caller's `Authorization`
/// and `Cookie` headers are not passed on.
#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub user_id: Option<i64>,
    pub method: String,
    /// The path after `/api/plugins/<name>/<subname>`, starting with `/`
    pub path: String,
    /// The query string without the `?`, empty when there is none
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// The first header called `name`, whatever its case
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// What the caller gets back
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// An empty answer with this status
    pub fn status(status: u16) -> Self {
        Self { status, headers: vec![], body: vec![] }
    }

    pub fn text(status: u16, text: impl Into<String>) -> Self {
        Self { status, headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())], body: text.into().into_bytes() }
    }

    pub fn json(status: u16, value: &Value) -> Self {
        Self { status, headers: vec![("content-type".into(), "application/json".into())], body: value.to_string().into_bytes() }
    }
}
