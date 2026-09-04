pub mod download_file;
pub mod request;
pub mod search_query;

use super::base::Tool;

/// Every tool in the `web` domain (function names prefixed `web.`), for `main.rs` to
/// register alongside every other domain's `collect()`.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(download_file::DownloadFileTool),
        Box::new(request::WebRequestTool),
        Box::new(search_query::SearchQueryTool),
    ]
}

/// Pulls the host out of a URL, for scoping a grant by host rather than by exact URL — a
/// grant for `example.com` never silently covers `localhost` or `169.254.169.254`, so
/// reaching an internal/local address (this backend runs `network_mode: host`, so it can
/// otherwise reach anything the host machine can) always surfaces its own explicit
/// approval prompt naming that exact host, rather than riding in on a broader grant.
pub(super) fn parse_host(url: &str) -> Option<String> {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string))
}
