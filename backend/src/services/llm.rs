use std::collections::HashMap;
use std::time::Duration;

use axum::http::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::services::error::ErrorService;
use crate::tools::base::Tool;

/// A request to Ollama gets no response at all until generation finishes (`stream:
/// false`) — without a client-side cap, a model that never emits a stop token (see
/// `split_thinking`'s "never closed" case) blocks the request indefinitely instead of
/// eventually failing. Scaled from `max_predict_tokens` rather than a fixed duration,
/// so it always covers a full legitimate max-length generation with margin — a flat
/// timeout shorter than `num_predict`'s worst case cuts off real, still-progressing
/// generations, not just genuinely stuck ones. `MIN_TOKENS_PER_SEC` is a conservative
/// floor, well under this project's ~8-9 t/s observed speed, to leave room for slower
/// hardware or a loaded system; `PROMPT_PROCESSING_BUFFER` covers prefill time on a
/// long conversation, which this floor doesn't otherwise account for.
const MIN_TOKENS_PER_SEC: f64 = 3.0;
const PROMPT_PROCESSING_BUFFER: Duration = Duration::from_secs(10 * 60);

fn request_timeout(max_predict_tokens: i32) -> Duration {
    Duration::from_secs_f64(max_predict_tokens as f64 / MIN_TOKENS_PER_SEC) + PROMPT_PROCESSING_BUFFER
}

/// Talks to Ollama's HTTP API and parses its responses into our own structs. Route
/// handlers go through this rather than calling Ollama directly, so the rest of the
/// app never has to know Ollama's wire format.
pub struct OllamaService {
    client: reqwest::Client,
    base_url: String,
    model_name: String,
    /// Hard ceiling on how many tokens a single generation can produce, sent as
    /// `options.num_predict` on every request. Ollama defaults `num_predict` to `-1`
    /// (unlimited) when it's omitted, so without this a model that never emits a stop
    /// token keeps generating indefinitely — the actual root cause of the
    /// runaway-generation incident that `REQUEST_TIMEOUT` above only band-aids over
    /// (that timeout stops the client from waiting forever, but does nothing to stop
    /// Ollama's own generation from continuing to run server-side after the client's
    /// given up). Sourced from `OLLAMA_CONTEXT_LENGTH` (`main.rs`) — matches Ollama's
    /// real context window by construction instead of a separately hardcoded number
    /// that could silently drift out of sync with it.
    max_predict_tokens: i32,
}

impl OllamaService {
    pub fn new(base_url: impl Into<String>, model_name: impl Into<String>, max_predict_tokens: i32) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(request_timeout(max_predict_tokens))
                .build()
                .expect("failed to build ollama http client"),
            base_url: base_url.into(),
            model_name: model_name.into(),
            max_predict_tokens,
        }
    }

    /// Maps the given tools into the JSON schema Ollama expects for a chat request's
    /// `tools` field (`{"type": "function", "function": {name, description, parameters:
    /// {...}}}`). Kept private and Ollama-specific — this schema is a detail of talking
    /// to Ollama, not something the rest of the app should know about.
    fn tool_definitions(tools: &[&dyn Tool]) -> Vec<OllamaToolDefinition> {
        tools
            .iter()
            .map(|tool| {
                let properties = tool
                    .required_properties()
                    .into_iter()
                    .map(|property| {
                        (
                            property.name,
                            OllamaToolProperty {
                                property_type: property.property_type.to_string(),
                                description: property.description,
                            },
                        )
                    })
                    .collect();

                OllamaToolDefinition {
                    definition_type: tool.tool_type().to_string(),
                    function: OllamaToolFunctionDefinition {
                        name: tool.function_name().to_string(),
                        description: tool.description().to_string(),
                        parameters: OllamaToolParameters {
                            parameters_type: "object".to_string(),
                            required: tool.required_param_names(),
                            properties,
                        },
                    },
                }
            })
            .collect()
    }


    /// Calls Ollama's legacy `/api/generate` endpoint: raw prompt string in, raw
    /// completion string out. No message roles, no chat history, no tool calls — kept
    /// around as the simplest possible path to a model response. `chat` below is the
    /// one actually meant for multi-turn/tool-calling use. `think` asks the model to
    /// reason before answering and defaults to `true` when not given; see
    /// `split_thinking` — this endpoint's `thinking` response field isn't populated for
    /// this model, so the reasoning trace comes back embedded in `response` instead and
    /// has to be pulled back out on our end.
    pub async fn generate(&self, prompt: String, think: Option<bool>) -> Result<OllamaGenerateResponse, OllamaErrors> {
        let url = format!("{}/api/generate", self.base_url);
        let body = OllamaGenerateRequest {
            model: self.model_name.clone(),
            prompt,
            stream: false,
            think: think.unwrap_or(true),
            options: OllamaOptions { num_predict: self.max_predict_tokens },
        };

        tracing::info!("calling ollama /api/generate");
        let res = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| OllamaErrors::RequestFailed(e.to_string()))?;

        // reqwest's `send` only errors on transport-level failures (connection refused,
        // timeout, TLS) — an HTTP error status like 500 still comes back as `Ok`. Without
        // this check, a non-2xx response (whose body likely isn't our expected JSON shape)
        // would surface as a confusing decode error instead of the actual status.
        if !res.status().is_success() {
            return Err(OllamaErrors::UnexpectedStatus(res.status()));
        }

        // `/api/generate` never populates `thinking` for this model (see the doc comment
        // above) — `ensure_thinking_split` always has something to do here.
        let mut result: OllamaGenerateResponse = decode_response(res).await?;
        log_ollama_metrics(&result.metrics);
        let (thinking, response) = ensure_thinking_split(result.thinking.take(), result.response);
        result.thinking = thinking;
        result.response = response;
        Ok(result)
    }

    /// Calls Ollama's `/api/chat` endpoint. `messages` is the conversation to send, in
    /// order. `new_message`, when given, is appended after `messages` as the turn being
    /// added now — kept as a separate parameter rather than requiring callers to fold it
    /// into `messages` themselves, since not every call has a new turn to add (a caller
    /// asking the model to continue based on the existing history alone passes `None`).
    /// `tools` is whatever the caller wants advertised to the model for this call, mapped
    /// via `tool_definitions` and omitted entirely if empty. Assembling and executing on
    /// a `tool_calls` response is the caller's job. `think` asks the model to reason
    /// before answering and defaults to `true` when not given.
    pub async fn chat(
        &self,
        messages: Vec<OllamaChatMessage>,
        new_message: Option<OllamaChatMessage>,
        tools: &[&dyn Tool],
        think: Option<bool>,
    ) -> Result<OllamaChatResponse, OllamaErrors> {
        let url = format!("{}/api/chat", self.base_url);

        let definitions = Self::tool_definitions(tools);
        let tools = if definitions.is_empty() {
            None
        } else {
            Some(definitions)
        };

        let mut messages = messages;
        if let Some(new_message) = new_message {
            messages.push(new_message);
        }

        let body = OllamaChatRequest {
            model: self.model_name.clone(),
            messages,
            stream: false,
            think: think.unwrap_or(true),
            tools,
            options: OllamaOptions { num_predict: self.max_predict_tokens },
        };

        tracing::info!("calling ollama /api/chat");
        let res = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| OllamaErrors::RequestFailed(e.to_string()))?;

        if !res.status().is_success() {
            return Err(OllamaErrors::UnexpectedStatus(res.status()));
        }

        // `/api/chat` usually populates `message.thinking` correctly on its own (unlike
        // `/api/generate` — see its doc comment) and leaves `content` clean — but not
        // reliably: at least one observed response came back with `thinking` non-empty
        // while `content` *still* held the whole raw `<think>...</think>` block,
        // unsplit. `ensure_thinking_split` triggers on that marker actually being
        // present in `content`, not on whether `thinking` looks unset, so this gets
        // caught too.
        let mut result: OllamaChatResponse = decode_response(res).await?;
        log_ollama_metrics(&result.metrics);
        let (thinking, content) = ensure_thinking_split(result.message.thinking.take(), result.message.content);
        result.message.thinking = thinking;
        result.message.content = content;
        Ok(result)
    }

    /// Builds a `user`-role message from plain text, so callers can hand `chat` a
    /// history/new-message pair without constructing `OllamaChatMessage` by hand.
    pub fn user_message(content: String) -> OllamaChatMessage {
        OllamaChatMessage {
            role: "user".to_string(),
            content,
            tool_calls: None,
            tool_name: None,
            thinking: None,
        }
    }

    /// Builds a `system`-role message from plain text, so callers can hand `chat` a
    /// history/new-message pair without constructing `OllamaChatMessage` by hand.
    pub fn system_message(content: String) -> OllamaChatMessage {
        OllamaChatMessage {
            role: "system".to_string(),
            content,
            tool_calls: None,
            tool_name: None,
            thinking: None,
        }
    }

    /// Builds a `tool`-role message carrying a tool's result, so callers can hand `chat`
    /// a history/new-message pair without constructing `OllamaChatMessage` by hand.
    /// `content` is a JSON value (a tool's result) rather than a `String` since that's
    /// what `Tool::call_untyped` returns; Ollama's wire format wants it stringified.
    pub fn tool_message(tool_name: String, content: Value) -> OllamaChatMessage {
        OllamaChatMessage {
            role: "tool".to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_name: Some(tool_name),
            thinking: None,
        }
    }
}

/// Reads the response body as text before parsing it, rather than `res.json()`
/// directly, purely so a parse failure can still log what Ollama actually sent — a bare
/// `serde_json`/reqwest decode error on its own doesn't say what the body looked like,
/// which makes a schema drift between us and Ollama's actual API hard to diagnose from
/// the error message alone.
async fn decode_response<T: DeserializeOwned>(res: reqwest::Response) -> Result<T, OllamaErrors> {
    let body_text = res
        .text()
        .await
        .map_err(|e| OllamaErrors::DecodeFailed(e.to_string()))?;

    serde_json::from_str(&body_text).map_err(|e| {
        tracing::error!("failed to decode ollama response: {e}\nbody = {body_text}");
        OllamaErrors::DecodeFailed(e.to_string())
    })
}

/// Splits a `</think>`-delimited reasoning block out of raw model output. Ollama's own
/// `thinking` response field depends on it recognizing the model's chat template as
/// thinking-aware; this model's template isn't tagged that way, so with `think: true`
/// the reasoning trace comes back embedded directly in the text instead — this pulls it
/// back out so callers get a clean answer plus the reasoning separately, regardless of
/// which path actually produced it. The opening `<think>` tag is injected by the chat
/// template as part of the assistant turn's preamble *before* generation starts, so it's
/// only present in the templated prompt, never in the model's actual output text — only
/// the model-emitted `</think>` reliably shows up, and everything before it is the
/// reasoning. If an explicit `<think>` tag is present too, text before it is preserved as
/// part of the response rather than folded into the reasoning. Returns `(None, text
/// unchanged)` when no `</think>` is present at all (e.g. the response got cut off
/// mid-thought) — in that case there's nothing safe to split, so the raw text is left
/// alone rather than guessing.
fn split_thinking(text: &str) -> (Option<String>, String) {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";

    let Some(end) = text.find(CLOSE) else {
        return (None, text.to_string());
    };

    let (prefix, thinking_start) = match text.find(OPEN) {
        Some(open) => (&text[..open], open + OPEN.len()),
        None => ("", 0),
    };

    let thinking = text[thinking_start..end].trim().to_string();
    let content = format!("{}{}", prefix, &text[end + CLOSE.len()..]);

    (Some(thinking), content.trim().to_string())
}

/// Guarantees `content` never still carries a raw `<think>...</think>` block once
/// this returns, filling in `thinking` from it if there wasn't one already. Ollama
/// doesn't reliably keep its promise to both populate `message.thinking` *and* strip
/// `content` for `/api/chat` — triggering purely on `thinking.is_none()` (the old
/// check) missed the case where it gave back a `thinking` value but left `content`
/// with the block still in it. Checking whether the marker is actually still present
/// in `content` catches that too. `existing_thinking` (Ollama's own, when it has one)
/// wins over what gets derived here — the derived value is only a fallback for
/// whenever Ollama didn't provide one at all.
fn ensure_thinking_split(existing_thinking: Option<String>, content: String) -> (Option<String>, String) {
    if !content.contains("</think>") {
        return (existing_thinking, content);
    }

    let (derived_thinking, clean_content) = split_thinking(&content);
    (existing_thinking.or(derived_thinking), clean_content)
}

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    think: bool,
    options: OllamaOptions,
}

/// Mirrors Ollama's `/api/generate` response shape, not our own API contract — route
/// handlers map the fields they need out of this into their own response types rather
/// than returning it directly, so Ollama's shape never leaks to callers.
#[derive(Deserialize)]
pub struct OllamaGenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    pub done: bool,
    /// Populated by `generate` after decoding, from `split_thinking` — not something
    /// Ollama's own JSON reliably fills in for this model (see `split_thinking`'s docs).
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(flatten)]
    metrics: OllamaMetrics,
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaChatMessage>,
    stream: bool,
    think: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaToolDefinition>>,
    options: OllamaOptions,
}

/// Generation options shared by both `/api/generate` and `/api/chat` requests. See
/// `OllamaService::max_predict_tokens`.
#[derive(Serialize)]
struct OllamaOptions {
    num_predict: i32,
}

/// One entry of the `tools` array in a chat request — the schema Ollama forwards to the
/// model so it knows what's callable. See `OllamaService::tool_definitions`.
#[derive(Serialize)]
struct OllamaToolDefinition {
    #[serde(rename = "type")]
    definition_type: String,
    function: OllamaToolFunctionDefinition,
}

#[derive(Serialize)]
struct OllamaToolFunctionDefinition {
    name: String,
    description: String,
    parameters: OllamaToolParameters,
}

#[derive(Serialize)]
struct OllamaToolParameters {
    #[serde(rename = "type")]
    parameters_type: String,
    required: Vec<String>,
    properties: HashMap<String, OllamaToolProperty>,
}

#[derive(Serialize)]
struct OllamaToolProperty {
    #[serde(rename = "type")]
    property_type: String,
    description: String,
}

/// Mirrors Ollama's `/api/chat` response shape — see `OllamaGenerateResponse` for why
/// this isn't reused as-is for our own API responses.
#[derive(Deserialize)]
pub struct OllamaChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: OllamaChatMessage,
    pub done: bool,
    #[serde(default)]
    pub done_reason: Option<String>,
    #[serde(flatten)]
    metrics: OllamaMetrics,
}

impl OllamaChatResponse {
    /// How many tokens this call's prompt used, if Ollama reported it — the real
    /// number (not an estimate), used to decide whether a chat's history is getting
    /// close enough to the context ceiling to be worth compacting. `None` on the rare
    /// response that omits metrics entirely, same as `log_ollama_metrics` already
    /// tolerates.
    pub fn prompt_eval_count(&self) -> Option<u64> {
        self.metrics.prompt_eval_count
    }
}

/// Timing/count fields Ollama includes on every non-streamed `/api/generate` and
/// `/api/chat` response. Kept private to this module and off both response structs'
/// public surface (not `pub`) — this is purely for `log_ollama_metrics` below, never
/// meant to reach a route handler or leak out over our own API.
#[derive(Deserialize, Default)]
struct OllamaMetrics {
    #[serde(default)]
    load_duration: Option<u64>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
    #[serde(default)]
    prompt_eval_duration: Option<u64>,
    #[serde(default)]
    eval_count: Option<u64>,
    #[serde(default)]
    eval_duration: Option<u64>,
    #[serde(default)]
    total_duration: Option<u64>,
}

/// Logs the per-request timing Ollama reports (all durations are nanoseconds on the
/// wire, converted to seconds here) plus a derived tokens/second figure for the
/// generation phase — the number that actually answers "is this slow because of a lot
/// of prompt/tools, or because the model itself is just decoding slowly."
fn log_ollama_metrics(metrics: &OllamaMetrics) {
    let tokens_per_second = match (metrics.eval_count, metrics.eval_duration) {
        (Some(count), Some(duration_ns)) if duration_ns > 0 => {
            Some(count as f64 / (duration_ns as f64 / 1e9))
        }
        _ => None,
    };

    tracing::info!(
        load_duration_s = metrics.load_duration.map(|ns| ns as f64 / 1e9),
        prompt_eval_count = metrics.prompt_eval_count,
        prompt_eval_duration_s = metrics.prompt_eval_duration.map(|ns| ns as f64 / 1e9),
        eval_count = metrics.eval_count,
        eval_duration_s = metrics.eval_duration.map(|ns| ns as f64 / 1e9),
        total_duration_s = metrics.total_duration.map(|ns| ns as f64 / 1e9),
        tokens_per_second,
        "ollama call finished"
    );
}

// Shared between outgoing request messages and the response's `message` field —
// covers what a message looks like on either side of the wire.
#[derive(Serialize, Deserialize, Clone)]
pub struct OllamaChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
    /// Only present on `tool`-role messages — which tool produced `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Populated by `chat` after decoding, from `split_thinking` — always `None` on
    /// outgoing messages we construct ourselves (`user_message`/`tool_message`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OllamaToolCall {
    pub id: String,
    pub function: OllamaToolCallFunction,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OllamaToolCallFunction {
    #[serde(default)]
    pub index: Option<u32>,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Failure modes for a call to Ollama, kept distinct so `From<OllamaErrors> for
/// ErrorService` below can map each to an appropriate HTTP status rather than
/// collapsing everything to a generic 500.
pub enum OllamaErrors {
    RequestFailed(String),
    UnexpectedStatus(StatusCode),
    DecodeFailed(String),
}

impl From<OllamaErrors> for ErrorService {
    fn from(err: OllamaErrors) -> Self {
        match err {
            OllamaErrors::RequestFailed(msg) => {
                // Transport-level failure (connection refused, timeout, ...) — not
                // something a caller did wrong, so the precise cause only matters here,
                // in the logs, not in the 500 body they get back.
                tracing::error!("failed to reach ollama: {msg}");
                ErrorService::internal(format!("failed to reach ollama: {msg}"))
            }
            OllamaErrors::UnexpectedStatus(code) => ErrorService::new(
                StatusCode::BAD_GATEWAY,
                format!("ollama returned status {code}"),
            ),
            OllamaErrors::DecodeFailed(msg) => {
                // The precise cause (including the raw body Ollama sent) is already
                // logged at the source in `decode_response` — this only has the
                // stringified message left, which isn't useful to log twice.
                ErrorService::internal(format!("failed to decode ollama response: {msg}"))
            }
        }
    }
}
