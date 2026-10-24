//! The plugin's end of the conversation: reads the backend's requests, answers them, and carries the plugin's own calls.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::stdio::{Input, Output};
use crate::{CallContext, Error, Failure, FailureKind, Host, HttpRequest, Plugin, PROTOCOL, UploadedFile};

const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL: i64 = -32603;
const BAD_ARGS: i64 = -32002;
const NOT_FOUND: i64 = -32001;

pub struct Connection {
    out: mpsc::UnboundedSender<String>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    next_id: AtomicU64,
    info: OnceLock<Value>,
}

impl Connection {
    /// Asks the backend and waits for its answer
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string();
        self.out.send(line).map_err(|_| Error("the backend closed the connection".into()))?;
        match rx.await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(message)) => Err(Error(message)),
            Err(_) => Err(Error("the backend closed the connection".into())),
        }
    }

    /// What the backend answered to `hello`
    pub fn host_info(&self) -> Option<Value> {
        self.info.get().cloned()
    }
}

pub async fn run(plugin: Arc<dyn Plugin>, stdin: Input, stdout: Output) -> Result<(), Error> {
    let (out, lines) = mpsc::unbounded_channel::<String>();
    let conn = Arc::new(Connection { out, pending: Mutex::default(), next_id: AtomicU64::new(1), info: OnceLock::new() });
    tokio::spawn(write_lines(stdout, lines));

    let reader = tokio::spawn(read_requests(BufReader::new(stdin), conn.clone(), plugin));

    let hello = json!({"protocol": PROTOCOL, "sdk": "rust"});
    let answer = conn.call("hello", hello).await.map_err(|e| Error(format!("the backend refused hello: {e}")))?;
    let _ = conn.info.set(answer);

    // Until the backend closes our stdin or asks us to shut down (which ends the process from `answer`)
    reader.await.map_err(|e| Error(format!("the reader stopped: {e}")))?;
    Ok(())
}

async fn write_lines(mut stdout: Output, mut lines: mpsc::UnboundedReceiver<String>) {
    while let Some(line) = lines.recv().await {
        if stdout.write_all(line.as_bytes()).await.is_err() || stdout.write_all(b"\n").await.is_err() || stdout.flush().await.is_err() {
            break;
        }
    }
}

async fn read_requests(mut stdin: BufReader<Input>, conn: Arc<Connection>, plugin: Arc<dyn Plugin>) {
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            eprintln!("tulpa-plugin: a line from the backend is not JSON, ignored");
            continue;
        };
        if message.get("method").is_some() {
            let (conn, plugin) = (conn.clone(), plugin.clone());
            tokio::spawn(async move { answer(conn, plugin, message).await });
        } else if let Some(id) = message["id"].as_u64() {
            let result = match message.get("error") {
                Some(e) => Err(e["message"].as_str().unwrap_or("the backend reported an error").to_string()),
                None => Ok(message["result"].clone()),
            };
            if let Some(tx) = conn.pending.lock().unwrap().remove(&id) {
                let _ = tx.send(result);
            }
        }
    }
    // The backend closed our stdin: it is done with us. Anything still waiting for an answer never gets one.
    conn.pending.lock().unwrap().clear();
}

async fn answer(conn: Arc<Connection>, plugin: Arc<dyn Plugin>, message: Value) {
    let method = message["method"].as_str().unwrap_or_default().to_string();
    let result = dispatch(&conn, &plugin, &method, message.get("params").cloned().unwrap_or(Value::Null)).await;
    let Some(id) = message.get("id") else { return };
    let line = match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, text)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": text}}),
    };
    let _ = conn.out.send(line.to_string());
    if method == "shutdown" {
        // The answer has to leave before the process does
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        std::process::exit(0);
    }
}

async fn dispatch(conn: &Arc<Connection>, plugin: &Arc<dyn Plugin>, method: &str, params: Value) -> Result<Value, (i64, String)> {
    match method {
        "tools" => Ok(serde_json::to_value(plugin.tools()).map_err(|e| (INTERNAL, e.to_string()))?),
        "call_tool" => {
            let tool = params["tool"].as_str().ok_or((INVALID_PARAMS, "call_tool needs a tool".to_string()))?.to_string();
            let args = params.get("args").cloned().unwrap_or(Value::Null);
            let ctx = &params["ctx"];
            let ctx = CallContext {
                user_id: ctx["user_id"].as_i64().ok_or((INVALID_PARAMS, "ctx.user_id is missing".to_string()))?,
                chat_id: ctx["chat_id"].as_i64().unwrap_or_default(),
                model: ctx["model"].as_str().unwrap_or_default().to_string(),
                provider: ctx["provider"].as_str().unwrap_or_default().to_string(),
                host: Host { conn: conn.clone() },
            };
            // On a task of its own so that a panic in the tool is an error answer, not the end of the plugin
            let plugin = plugin.clone();
            match tokio::spawn(async move { plugin.call_tool(&tool, args, ctx).await }).await {
                Ok(result) => failed(result),
                Err(_) => Err((INTERNAL, "the tool panicked".to_string())),
            }
        }
        "configure" => {
            let settings = params.get("settings").cloned().unwrap_or(Value::Null);
            failed(plugin.configure(settings, Host { conn: conn.clone() }).await.map(|()| Value::Null))
        }
        "enable" => failed(plugin.enable().await.map(|()| Value::Null)),
        "disable" => failed(plugin.disable().await.map(|()| Value::Null)),
        "prompt_section" => {
            let (user_id, tools) = (user_id(&params)?, params["tools"].as_bool().unwrap_or(true));
            Ok(plugin.prompt_section(user_id, tools).await.map_or(Value::Null, Value::String))
        }
        "tab_layout" => tab(plugin.tab_layout(user_id(&params)?).await),
        "tab_action" => {
            let action = params["action"].as_str().ok_or((INVALID_PARAMS, "tab_action needs an action".to_string()))?;
            tab(plugin.tab_action(user_id(&params)?, action, params.get("args").cloned().unwrap_or(Value::Null)).await)
        }
        "tab_upload" => {
            let action = params["action"].as_str().ok_or((INVALID_PARAMS, "tab_upload needs an action".to_string()))?;
            let files = params["files"]
                .as_array()
                .ok_or((INVALID_PARAMS, "tab_upload needs files".to_string()))?
                .iter()
                .filter_map(|f| Some(UploadedFile { path: f["path"].as_str()?.to_string(), file: f["file"].as_str()?.into() }))
                .collect();
            tab(plugin.tab_upload(user_id(&params)?, action, params["row"].as_str().map(str::to_string), files).await)
        }
        "http_request" => {
            let body = STANDARD.decode(params["body_b64"].as_str().unwrap_or_default()).map_err(|e| (INVALID_PARAMS, format!("body_b64: {e}")))?;
            let pairs = |value: &Value| -> Vec<(String, String)> {
                value.as_array().into_iter().flatten().filter_map(|h| Some((h[0].as_str()?.to_string(), h[1].as_str()?.to_string()))).collect()
            };
            let request = HttpRequest {
                user_id: params["user_id"].as_i64(),
                method: params["method"].as_str().unwrap_or("GET").to_string(),
                path: params["path"].as_str().unwrap_or("/").to_string(),
                query: params["query"].as_str().unwrap_or_default().to_string(),
                headers: pairs(&params["headers"]),
                body,
            };
            // On a task of its own, like a tool call: a panic is an error answer, not the end of the plugin
            let plugin = plugin.clone();
            let response = tokio::spawn(async move { plugin.http_request(request).await }).await.map_err(|_| (INTERNAL, "the route panicked".to_string()))?;
            Ok(json!({"status": response.status, "headers": response.headers, "body_b64": STANDARD.encode(response.body)}))
        }
        "user_deleted" => {
            plugin.user_deleted(user_id(&params)?).await;
            Ok(Value::Null)
        }
        "shutdown" => Ok(Value::Null),
        _ => Err((METHOD_NOT_FOUND, format!("the plugin has no method {method}"))),
    }
}

fn user_id(params: &Value) -> Result<i64, (i64, String)> {
    params["user_id"].as_i64().ok_or((INVALID_PARAMS, "user_id is missing".to_string()))
}

fn failed(result: Result<Value, Failure>) -> Result<Value, (i64, String)> {
    result.map_err(|Failure { kind, message }| {
        (
            match kind {
                FailureKind::BadArgs => BAD_ARGS,
                FailureKind::NotFound => NOT_FOUND,
                FailureKind::Internal => INTERNAL,
            },
            message,
        )
    })
}

fn tab(result: Result<crate::Tab, Failure>) -> Result<Value, (i64, String)> {
    failed(result.and_then(|tab| serde_json::to_value(tab).map_err(|e| Failure::from(e.to_string()))))
}
