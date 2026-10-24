//! JSON-RPC 2.0 over a pair of byte streams, one JSON object per line: the transport of `plugin-sdk/PROTOCOL.md`. Both
//! sides ask and both answer. `RpcClient` is the host's end: it writes requests, matches the answers that come back by id,
//! and passes the plugin's own requests to a handler.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// The longest line either side may send. A bigger payload goes through a file in the plugin's data folder.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
/// How long a call waits for its answer when the caller names no other limit
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(30);

pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INTERNAL: i64 = -32603;
pub const BAD_ARGS: i64 = -32002;

/// An error as it travels in a response
#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }
}

/// Why a call produced no result
#[derive(Debug)]
pub enum CallError {
    /// The plugin answered with an error
    Rpc(RpcError),
    /// No answer within the time given
    Timeout,
    /// The connection is gone (the process exited or closed its stdout)
    Closed,
}

/// What the host does with a request or notification the plugin sends. A notification's result is dropped.
#[async_trait]
pub trait Incoming: Send + Sync + 'static {
    async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError>;
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, CallError>>>>>;

pub struct RpcClient {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    closed: Arc<AtomicBool>,
    stop: CancellationToken,
}

impl RpcClient {
    /// Starts the reader and writer tasks over `reader` (the plugin's stdout) and `writer` (its stdin). `label` names the
    /// plugin in the log.
    pub fn start<R, W>(reader: R, writer: W, handler: Arc<dyn Incoming>, label: String) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        Self::start_with_limit(reader, writer, handler, label, MAX_MESSAGE_BYTES)
    }

    fn start_with_limit<R, W>(reader: R, writer: W, handler: Arc<dyn Incoming>, label: String, max_bytes: usize) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (out, lines) = mpsc::unbounded_channel::<String>();
        let pending: Pending = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        let stop = CancellationToken::new();

        tokio::spawn(write_lines(writer, lines, stop.clone()));
        tokio::spawn(read_lines(BufReader::new(reader), handler, out.clone(), pending.clone(), closed.clone(), label, max_bytes));

        Self { out, pending, next_id: AtomicU64::new(1), closed, stop }
    }

    /// Asks the plugin and waits for its answer, at most `timeout`
    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, CallError> {
        if self.is_closed() {
            return Err(CallError::Closed);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        if self.out.send(line).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(CallError::Closed);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(CallError::Closed),
            Err(_) => {
                // An answer that arrives later finds no entry and is dropped
                self.pending.lock().unwrap().remove(&id);
                Err(CallError::Timeout)
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    /// Stops writing and closes the plugin's stdin, which is how it learns that the backend is done with it
    pub fn close(&self) {
        self.stop.cancel();
    }
}

async fn write_lines<W: AsyncWrite + Unpin>(mut writer: W, mut lines: mpsc::UnboundedReceiver<String>, stop: CancellationToken) {
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            line = lines.recv() => {
                let Some(line) = line else { break };
                // A plugin that closed its end is noticed by the reader; nothing more to do here
                if writer.write_all(line.as_bytes()).await.is_err() || writer.write_all(b"\n").await.is_err() || writer.flush().await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = writer.shutdown().await;
}

/// One line without its newline. `Ok(None)` at the end of the stream; a line over `max_bytes` is read to its end and
/// reported as `Err(())` so the next one can still be used.
pub(super) async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R, max_bytes: usize) -> std::io::Result<Option<Result<Vec<u8>, ()>>> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let (consumed, done) = {
            let chunk = reader.fill_buf().await?;
            if chunk.is_empty() {
                // The stream ended; a half line is not a message
                return Ok(None);
            }
            match chunk.iter().position(|&b| b == b'\n') {
                Some(end) => {
                    if !too_long && line.len() + end <= max_bytes {
                        line.extend_from_slice(&chunk[..end]);
                    } else {
                        too_long = true;
                    }
                    (end + 1, true)
                }
                None => {
                    if !too_long && line.len() + chunk.len() <= max_bytes {
                        line.extend_from_slice(chunk);
                    } else {
                        too_long = true;
                        line.clear();
                    }
                    (chunk.len(), false)
                }
            }
        };
        reader.consume(consumed);
        if done {
            return Ok(Some(if too_long { Err(()) } else { Ok(line) }));
        }
    }
}

async fn read_lines<R: AsyncBufRead + Unpin>(
    mut reader: R,
    handler: Arc<dyn Incoming>,
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    closed: Arc<AtomicBool>,
    label: String,
    max_bytes: usize,
) {
    loop {
        let line = match read_line(&mut reader, max_bytes).await {
            Ok(Some(Ok(line))) => line,
            Ok(Some(Err(()))) => {
                tracing::warn!("plugin {label}: a message over {max_bytes} bytes was dropped");
                continue;
            }
            Ok(None) | Err(_) => break,
        };
        if line.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        let Ok(message) = serde_json::from_slice::<Value>(&line) else {
            // A stray print on stdout: not part of the protocol, said once and skipped
            tracing::warn!("plugin {label}: stdout line is not JSON, ignored: {}", String::from_utf8_lossy(&line[..line.len().min(200)]));
            continue;
        };
        if message.get("method").is_some() {
            let handler = handler.clone();
            let out = out.clone();
            let label = label.clone();
            tokio::spawn(async move { answer_request(handler, out, message, &label).await });
        } else if message.get("id").is_some() && (message.get("result").is_some() || message.get("error").is_some()) {
            let result = match message.get("error") {
                Some(e) => Err(CallError::Rpc(RpcError {
                    code: e.get("code").and_then(Value::as_i64).unwrap_or(INTERNAL),
                    message: e.get("message").and_then(Value::as_str).unwrap_or("the plugin reported an error").to_string(),
                    data: e.get("data").cloned(),
                })),
                None => Ok(message["result"].clone()),
            };
            let waiting = message["id"].as_u64().and_then(|id| pending.lock().unwrap().remove(&id));
            match waiting {
                Some(tx) => {
                    let _ = tx.send(result);
                }
                None => tracing::debug!("plugin {label}: an answer nobody is waiting for (late or unknown id), ignored"),
            }
        } else {
            tracing::warn!("plugin {label}: a message that is neither a request nor an answer, ignored");
        }
    }
    closed.store(true, Ordering::Relaxed);
    for (_, tx) in pending.lock().unwrap().drain() {
        let _ = tx.send(Err(CallError::Closed));
    }
}

async fn answer_request(handler: Arc<dyn Incoming>, out: mpsc::UnboundedSender<String>, message: Value, label: &str) {
    let Some(method) = message["method"].as_str() else {
        if message.get("id").is_some() {
            let _ = out.send(error_line(&message["id"], &RpcError::new(INVALID_REQUEST, "method must be a string")));
        }
        return;
    };
    let result = handler.handle(method, message.get("params").cloned().unwrap_or(Value::Null)).await;
    // No id: a notification, which gets no answer
    let Some(id) = message.get("id") else {
        if let Err(e) = result {
            tracing::debug!("plugin {label}: notification {method} failed: {}", e.message);
        }
        return;
    };
    let line = match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string(),
        Err(e) => error_line(id, &e),
    };
    let _ = out.send(line);
}

fn error_line(id: &Value, error: &RpcError) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": error}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{DuplexStream, duplex};

    struct Echo;

    #[async_trait]
    impl Incoming for Echo {
        async fn handle(&self, method: &str, params: Value) -> Result<Value, RpcError> {
            match method {
                "echo" => Ok(params),
                _ => Err(RpcError::new(METHOD_NOT_FOUND, format!("no method {method}"))),
            }
        }
    }

    /// The host end, plus the plugin's side as two raw streams the test plays by hand
    fn pair(max_bytes: usize) -> (RpcClient, BufReader<DuplexStream>, DuplexStream) {
        let (host_out, plugin_in) = duplex(1 << 20);
        let (plugin_out, host_in) = duplex(1 << 20);
        let client = RpcClient::start_with_limit(host_in, host_out, Arc::new(Echo), "test".into(), max_bytes);
        (client, BufReader::new(plugin_in), plugin_out)
    }

    async fn next_request(plugin_in: &mut BufReader<DuplexStream>) -> Value {
        let mut line = String::new();
        plugin_in.read_line(&mut line).await.unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[tokio::test]
    async fn answers_are_matched_by_id_whatever_order_they_come_in() {
        let (client, mut plugin_in, mut plugin_out) = pair(MAX_MESSAGE_BYTES);
        let client = Arc::new(client);
        let (a, b) = (client.clone(), client.clone());
        let first = tokio::spawn(async move { a.call("first", json!({}), Duration::from_secs(5)).await });
        let second = tokio::spawn(async move { b.call("second", json!({}), Duration::from_secs(5)).await });
        let (r1, r2) = (next_request(&mut plugin_in).await, next_request(&mut plugin_in).await);
        for request in [&r2, &r1] {
            let line = json!({"jsonrpc": "2.0", "id": request["id"], "result": request["method"]}).to_string();
            plugin_out.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        }
        assert_eq!(first.await.unwrap().unwrap(), json!("first"));
        assert_eq!(second.await.unwrap().unwrap(), json!("second"));
    }

    #[tokio::test]
    async fn an_error_answer_keeps_its_code_and_text() {
        let (client, mut plugin_in, mut plugin_out) = pair(MAX_MESSAGE_BYTES);
        let call = tokio::spawn(async move { client.call("x", json!({}), Duration::from_secs(5)).await });
        let request = next_request(&mut plugin_in).await;
        let line = json!({"jsonrpc": "2.0", "id": request["id"], "error": {"code": BAD_ARGS, "message": "no good"}}).to_string();
        plugin_out.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        match call.await.unwrap() {
            Err(CallError::Rpc(e)) => assert_eq!((e.code, e.message.as_str()), (BAD_ARGS, "no good")),
            other => panic!("expected an rpc error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_call_times_out_and_a_late_answer_does_no_harm() {
        let (client, mut plugin_in, mut plugin_out) = pair(MAX_MESSAGE_BYTES);
        let result = client.call("slow", json!({}), Duration::from_millis(50)).await;
        assert!(matches!(result, Err(CallError::Timeout)));
        let request = next_request(&mut plugin_in).await;
        let late = json!({"jsonrpc": "2.0", "id": request["id"], "result": 1}).to_string();
        plugin_out.write_all(format!("{late}\n").as_bytes()).await.unwrap();
        // The connection still works afterwards
        let call = tokio::spawn(async move { client.call("again", json!({}), Duration::from_secs(5)).await });
        let request = next_request(&mut plugin_in).await;
        let line = json!({"jsonrpc": "2.0", "id": request["id"], "result": "fine"}).to_string();
        plugin_out.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        assert_eq!(call.await.unwrap().unwrap(), json!("fine"));
    }

    #[tokio::test]
    async fn a_request_from_the_plugin_is_handled_and_answered() {
        let (_client, mut plugin_in, mut plugin_out) = pair(MAX_MESSAGE_BYTES);
        plugin_out.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"abc\",\"method\":\"echo\",\"params\":{\"n\":7}}\n").await.unwrap();
        plugin_out.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"nope\"}\n").await.unwrap();
        let (a, b) = (next_request(&mut plugin_in).await, next_request(&mut plugin_in).await);
        let (ok, err) = if a["id"] == "abc" { (a, b) } else { (b, a) };
        assert_eq!(ok["result"], json!({"n": 7}));
        assert_eq!((err["id"].clone(), err["error"]["code"].clone()), (json!(9), json!(METHOD_NOT_FOUND)));
    }

    #[tokio::test]
    async fn a_line_that_is_not_json_or_too_long_is_skipped_and_the_next_one_still_works() {
        let (client, mut plugin_in, mut plugin_out) = pair(200);
        plugin_out.write_all(b"just a print\n").await.unwrap();
        plugin_out.write_all(format!("{}\n", "x".repeat(5000)).as_bytes()).await.unwrap();
        let call = tokio::spawn(async move { client.call("x", json!({}), Duration::from_secs(5)).await });
        let request = next_request(&mut plugin_in).await;
        let line = json!({"jsonrpc": "2.0", "id": request["id"], "result": "ok"}).to_string();
        plugin_out.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        assert_eq!(call.await.unwrap().unwrap(), json!("ok"));
    }

    #[tokio::test]
    async fn a_closed_stream_fails_what_was_waiting_and_what_comes_after() {
        let (client, mut plugin_in, plugin_out) = pair(MAX_MESSAGE_BYTES);
        let client = Arc::new(client);
        let waiting = {
            let client = client.clone();
            tokio::spawn(async move { client.call("x", json!({}), Duration::from_secs(5)).await })
        };
        next_request(&mut plugin_in).await;
        drop(plugin_out);
        assert!(matches!(waiting.await.unwrap(), Err(CallError::Closed)));
        assert!(matches!(client.call("y", json!({}), Duration::from_secs(1)).await, Err(CallError::Closed)));
    }

    #[tokio::test]
    async fn closing_the_client_closes_the_plugins_stdin() {
        let (client, mut plugin_in, _plugin_out) = pair(MAX_MESSAGE_BYTES);
        client.close();
        let mut rest = String::new();
        let read = tokio::time::timeout(Duration::from_secs(5), plugin_in.read_line(&mut rest)).await.unwrap().unwrap();
        assert_eq!(read, 0, "end of stream, no data");
    }
}
