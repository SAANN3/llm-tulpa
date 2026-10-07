pub mod download_file;
pub mod request;
pub mod search_query;

use std::time::Duration;

use super::base::{Tool, ToolError};

/// How long a server gets to accept the connection.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Redirects followed within one host before giving up.
const MAX_REDIRECTS: usize = 10;

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

/// The HTTP client a tool call uses. Redirects are followed only while they stay on the URL's own host: a
/// host is approved by name, and a server that answers with a redirect to `localhost` or a LAN address must not
/// carry the call there on that approval (the redirect comes back as the answer, with its `Location`, and a
/// request to that address asks for its own approval). Without timeouts a server that never answers held the
/// tool, and with it the whole run, forever: a running tool isn't interrupted by Stop.
pub(super) fn client_for(url: &str, timeout: Option<Duration>, read_timeout: Option<Duration>) -> Result<reqwest::Client, ToolError> {
    let host = parse_host(url);
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS || attempt.url().host_str() != host.as_deref() {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    let mut builder = reqwest::Client::builder().redirect(policy).connect_timeout(CONNECT_TIMEOUT);
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    if let Some(read_timeout) = read_timeout {
        builder = builder.read_timeout(read_timeout);
    }
    builder.build().map_err(|e| ToolError::FailedUnknown(format!("couldn't set up the HTTP client: {e}")))
}

/// Where a redirect that wasn't followed points, for the model to request (and the user to approve) itself.
pub(super) fn redirect_location(response: &reqwest::Response) -> Option<String> {
    if !response.status().is_redirection() {
        return None;
    }
    let location = response.headers().get(reqwest::header::LOCATION)?.to_str().ok()?;
    // A relative `Location` is resolved against the address that answered
    Some(response.url().join(location).map(|url| url.to_string()).unwrap_or_else(|_| location.to_string()))
}

/// A one-purpose HTTP server for the tests: answers each path with a canned response, or stalls.
#[cfg(test)]
pub(super) mod test_server {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Serves `respond(path) -> raw response bytes` (`None`: send headers and then nothing) until the test ends.
    pub(in crate::tools) async fn serve(respond: fn(&str, u16) -> Option<Vec<u8>>) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { return };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = socket.read(&mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                    match respond(&path, port) {
                        Some(bytes) => {
                            let _ = socket.write_all(&bytes).await;
                        }
                        None => {
                            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nstart").await;
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        }
                    }
                });
            }
        });
        port
    }

    pub(in crate::tools) fn ok(body: &[u8]) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
        out.extend_from_slice(body);
        out
    }

    pub(in crate::tools) fn redirect(location: &str) -> Vec<u8> {
        format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::{ok, redirect, serve};
    use super::*;

    fn routes(path: &str, port: u16) -> Option<Vec<u8>> {
        Some(match path {
            "/elsewhere" => redirect(&format!("http://localhost:{port}/secret")),
            "/moved" => redirect("/here"),
            "/here" => ok(b"here"),
            "/stall" => return None,
            _ => ok(b"secret"),
        })
    }

    #[tokio::test]
    async fn a_redirect_to_another_host_is_answered_not_followed() {
        let port = serve(routes).await;
        let url = format!("http://127.0.0.1:{port}/elsewhere");
        let response = client_for(&url, Some(Duration::from_secs(5)), None).ok().expect("client").get(&url).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 302);
        assert_eq!(redirect_location(&response).as_deref(), Some(format!("http://localhost:{port}/secret").as_str()));
    }

    #[tokio::test]
    async fn a_redirect_on_the_same_host_is_followed() {
        let port = serve(routes).await;
        let url = format!("http://127.0.0.1:{port}/moved");
        let response = client_for(&url, Some(Duration::from_secs(5)), None).ok().expect("client").get(&url).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(redirect_location(&response), None);
        assert_eq!(response.text().await.unwrap(), "here");
    }

    #[tokio::test]
    async fn a_server_that_stops_sending_is_given_up_on() {
        let port = serve(routes).await;
        let url = format!("http://127.0.0.1:{port}/stall");
        let started = std::time::Instant::now();
        let mut response = client_for(&url, None, Some(Duration::from_millis(500))).ok().expect("client").get(&url).send().await.unwrap();
        let mut result = Ok(None);
        for _ in 0..3 {
            result = response.chunk().await;
            if result.is_err() {
                break;
            }
        }
        assert!(result.is_err(), "the stalled body should time out");
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
