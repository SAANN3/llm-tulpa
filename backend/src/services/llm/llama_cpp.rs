//! llama.cpp's own server (`llama-server`) as a model provider, over its OpenAI-compatible
//! `/v1/chat/completions` and its `/props`, `/slots` and `/metrics` endpoints, converted to and
//! from the shared types in `types.rs`.
//!
//! Translation contract, confirmed empirically against a real server before it was written (see
//! project memory for the transcripts), not assumed from docs alone:
//!   - `think` -> `chat_template_kwargs.enable_thinking`; a graduated level is also sent, exactly as
//!     given, as `chat_template_kwargs.reasoning_effort`. Plain `true` sends no effort: forcing a
//!     default level was tried and reverted.
//!   - The reply-length cap -> `max_tokens` (the exact field that caps generation).
//!   - `tools` pass through unchanged: Ollama's tool-definition schema is OpenAI's.
//!   - Outgoing tool-call arguments (a real JSON value on our side) become the JSON *string* OpenAI
//!     expects; incoming ones are parsed back into a value, so downstream code sees real fields
//!     and not a string that happens to hold JSON.
//!   - `tool_call_id`, which OpenAI's tool-result messages require, is synthesized per assistant
//!     message and matched to the `tool` messages that follow it purely by order. That is safe
//!     because the agent never round-trips a real id: it matches results by tool name and position.
//!   - Images (base64, no data-URL prefix) become `image_url` content parts with a data URL.
//!   - `timings.{prompt,predicted}_ms` (float milliseconds) become nanosecond durations.

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::SecondsFormat;
use serde::Deserialize;
use serde_json::{json, Value};

use super::budget::OutputBudget;
use super::prefix_watch::PrefixWatch;
use super::provider::LlmProvider;
use super::template::{ensure_thinking_split, extract_tool_call_markers, parse_thinking_capability};
use super::tool_defs::{tool_definitions, ToolDefinition};
use super::types::{
    CallParams, ChatMessage, ChatResponse, GenerateResponse, LaunchRequest, LlamaServerStats, LlmErrors, LocalModel,
    LocalModelDetails, ModelToolCall, ModelToolCallFunction, ResponseMetrics, RunningModel, RunningModels, Sampling,
    ThinkChoice, ThinkingCapability,
};
use crate::services::error::ErrorService;
use crate::services::gguf::read_gguf_info;
use crate::services::llama_runtime::{CallGuard, LlamaRuntime};
use crate::services::model_folder::ModelFolder;
use crate::tools::base::Tool;

/// The name this provider is registered under (`llm_providers.name`) and labels its errors with.
pub const PROVIDER: &str = "llama-cpp";

pub struct LlamaCppProvider {
    runtime: Arc<LlamaRuntime>,
    /// Where the model files are, so a model's chat template can be read from its own header without
    /// the server running.
    model_dir: Arc<ModelFolder>,
    client: reqwest::Client,
    base_url: String,
    budget: OutputBudget,
    prefix_watch: PrefixWatch,
}

impl LlamaCppProvider {
    /// `default_context` is the context window used to cap a reply when a call doesn't name the
    /// model's own (`CallParams::context_length`).
    pub fn new(runtime: Arc<LlamaRuntime>, model_dir: Arc<ModelFolder>, base_url: impl Into<String>, default_context: u64) -> Self {
        Self {
            runtime,
            model_dir,
            // No client-wide timeout: each request carries one scaled to its own reply-length cap.
            client: reqwest::Client::builder().build().expect("failed to build llama.cpp http client"),
            base_url: base_url.into(),
            budget: OutputBudget::new(default_context),
            prefix_watch: PrefixWatch::default(),
        }
    }

    /// Sends one `/v1/chat/completions` request and returns the decoded reply.
    async fn complete(&self, payload: Value, num_predict: i32) -> Result<CompletionResponse, LlmErrors> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        tracing::info!("calling llama-server /v1/chat/completions");
        let res = self
            .client
            .post(&url)
            .timeout(OutputBudget::timeout_for(num_predict))
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "llama-server /v1/chat/completions request failed");
                LlmErrors::RequestFailed(PROVIDER, e.to_string())
            })?;

        // `send` only errors on transport failures: an HTTP error status still comes back `Ok`, and
        // its body isn't the shape decoded below.
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "llama-server /v1/chat/completions returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }
        let decoded: CompletionResponse = decode_response(res).await?;
        log_metrics(&decoded);
        Ok(decoded)
    }

    async fn get_text(&self, path: &str, timeout_secs: u64) -> Result<String, LlmErrors> {
        let url = format!("{}{path}", self.base_url);
        let res = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "llama-server {path} request failed");
                LlmErrors::RequestFailed(PROVIDER, e.to_string())
            })?;
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "llama-server {path} returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }
        res.text().await.map_err(|e| LlmErrors::DecodeFailed(PROVIDER, e.to_string()))
    }

    /// The model's chat template. Read from the model file's own header, so it is known without
    /// loading the model (opening a chat must not start the GPU); a running server's `/props` answers
    /// when the file can't be read (an external server, a model outside the model directory).
    ///
    /// `None` is an answer too: a readable file that carries no template has none, whatever a
    /// server would say.
    async fn template(&self, model: &str) -> Result<Option<String>, LlmErrors> {
        if let Some(dir) = self.model_dir.get() {
            let path = dir.join(model);
            let from_file = tokio::task::spawn_blocking(move || read_gguf_info(&path).ok().map(|info| info.chat_template))
                .await
                .ok()
                .flatten();
            if let Some(template) = from_file {
                return Ok(template);
            }
        }
        Ok(Some(self.props().await?.chat_template))
    }

    async fn props(&self) -> Result<Props, LlmErrors> {
        let text = self.get_text("/props", 30).await?;
        serde_json::from_str(&text).map_err(|e| {
            tracing::error!("failed to decode llama-server /props: {e}\nbody = {text}");
            LlmErrors::DecodeFailed(PROVIDER, e.to_string())
        })
    }
}

#[async_trait]
impl LlmProvider for LlamaCppProvider {
    fn name(&self) -> &'static str {
        PROVIDER
    }

    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        new_message: Option<ChatMessage>,
        tools: &[&dyn Tool],
        think: Option<ThinkChoice>,
        model: &str,
        known_prompt_tokens: Option<u64>,
        params: &CallParams,
    ) -> Result<ChatResponse, LlmErrors> {
        // Held per call even when the caller holds the turn, so a call that names no launch still
        // finds a server running.
        let _claim = self.acquire(params.launch.as_ref()).await.map_err(LlmErrors::Unavailable)?;
        let definitions = tool_definitions(tools);
        // The tool schemas are part of the prompt, so they cost context too: counted from the exact
        // JSON that goes on the wire.
        let tool_overhead_tokens = if definitions.is_empty() {
            0
        } else {
            OutputBudget::tool_overhead_tokens(serde_json::to_string(&definitions).unwrap_or_default().len())
        };
        let definitions = (!definitions.is_empty()).then_some(definitions);

        let mut messages = messages;
        if let Some(new_message) = new_message {
            messages.push(new_message);
        }

        let num_predict = self.budget.for_chat(&messages, tool_overhead_tokens, known_prompt_tokens, params.context_length);
        self.prefix_watch.check(model, &definitions, &messages);

        let payload = build_payload(model, openai_messages(&messages), think, num_predict, definitions.as_deref(), &params.sampling);
        let decoded = self.complete(payload, num_predict).await?;
        Ok(decoded.into_chat_response(model))
    }

    async fn generate(
        &self,
        prompt: String,
        think: Option<bool>,
        model: &str,
        params: &CallParams,
    ) -> Result<GenerateResponse, LlmErrors> {
        let _claim = self.acquire(params.launch.as_ref()).await.map_err(LlmErrors::Unavailable)?;
        let num_predict = self.budget.for_generate(&prompt, params.context_length);
        let messages = vec![json!({"role": "user", "content": prompt})];
        let think = Some(ThinkChoice::Enabled(think.unwrap_or(true)));
        let decoded = self.complete(build_payload(model, messages, think, num_predict, None, &params.sampling), num_predict).await?;

        let message = decoded.choices.into_iter().next().map(|c| c.message).unwrap_or_default();
        let (thinking, response) =
            ensure_thinking_split(message.reasoning_content.filter(|t| !t.is_empty()), message.content.unwrap_or_default());
        Ok(GenerateResponse { model: model.to_string(), created_at: now_rfc3339(), response, thinking })
    }

    async fn acquire(&self, launch: Option<&LaunchRequest>) -> Result<CallGuard, ErrorService> {
        self.runtime.acquire(launch).await
    }

    async fn thinking_capability(&self, model: &str) -> Result<ThinkingCapability, LlmErrors> {
        Ok(match self.template(model).await? {
            Some(template) => parse_thinking_capability(&template),
            None => ThinkingCapability::Unsupported,
        })
    }

    async fn tool_call_markers(&self, model: &str) -> Vec<String> {
        match self.template(model).await {
            Ok(Some(template)) => extract_tool_call_markers(&template),
            Ok(None) | Err(_) => vec![],
        }
    }

    /// The one model a running llama-server serves: it has no registry to list.
    async fn list_local_models(&self) -> Result<Vec<LocalModel>, LlmErrors> {
        let props = self.props().await?;
        let name = props.model_file().unwrap_or_else(|| "local".to_string());
        let quantization_level = props.model_ftype.as_deref().map(|f| f.split(" - ").next().unwrap_or(f).trim().to_string());
        Ok(vec![LocalModel {
            name,
            size: None,
            details: Some(LocalModelDetails { family: None, parameter_size: None, quantization_level }),
        }])
    }

    /// What the server has loaded (always exactly its one model while it runs), its context, and what
    /// it has done since it started. `/slots` and `/metrics` are optional on the server's side, so each
    /// is skipped when it isn't served rather than failing the call.
    async fn running_models(&self) -> Result<RunningModels, LlmErrors> {
        let props = self.props().await?;
        let name = props.model_file().unwrap_or_else(|| "local".to_string());
        let context_length = props.default_generation_settings.as_ref().and_then(|g| g.n_ctx);

        let mut stats = LlamaServerStats {
            model_file: props.model_file(),
            slots_total: props.total_slots,
            ..Default::default()
        };
        if let Ok(text) = self.get_text("/slots", 5).await {
            if let Ok(slots) = serde_json::from_str::<Vec<Value>>(&text) {
                stats.slots_processing =
                    Some(slots.iter().filter(|s| s.get("is_processing").and_then(Value::as_bool) == Some(true)).count() as u64);
            }
        }
        if let Ok(text) = self.get_text("/metrics", 5).await {
            let gauges = prometheus_gauges(&text);
            // Lifetime averages from the totals: the server's own "per second" gauges describe only its
            // last measuring window, so they read 0 whenever it is idle.
            let per_second = |tokens: &str, seconds: &str| {
                let total_seconds = gauges.get(&format!("llamacpp:{seconds}")).copied().unwrap_or(0.0);
                (total_seconds > 0.0).then(|| gauges.get(&format!("llamacpp:{tokens}")).copied().unwrap_or(0.0) / total_seconds)
            };
            stats.prompt_tokens_total = gauges.get("llamacpp:prompt_tokens_total").copied();
            stats.prompt_tokens_cached_total = gauges.get("llamacpp:prompt_tokens_cached_total").copied();
            stats.prompt_tokens_per_second = per_second("prompt_tokens_total", "prompt_seconds_total");
            stats.predicted_tokens_total = gauges.get("llamacpp:tokens_predicted_total").copied();
            stats.predicted_tokens_per_second = per_second("tokens_predicted_total", "tokens_predicted_seconds_total");
            stats.draft_tokens_total = gauges.get("llamacpp:spec_decode_num_draft_tokens_total").copied();
            stats.draft_tokens_accepted_total = gauges.get("llamacpp:spec_decode_num_accepted_tokens_total").copied();
        }

        Ok(RunningModels {
            models: vec![RunningModel { name, size: None, size_vram: None, expires_at: None, context_length }],
            llama_server: Some(stats),
        })
    }
}

/// Reads the response body as text before parsing it, so a parse failure can still log what the
/// server actually sent — a bare decode error doesn't say what the body looked like.
async fn decode_response<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> Result<T, LlmErrors> {
    let body_text = res.text().await.map_err(|e| LlmErrors::DecodeFailed(PROVIDER, e.to_string()))?;
    serde_json::from_str(&body_text).map_err(|e| {
        tracing::error!("failed to decode llama-server response: {e}\nbody = {body_text}");
        LlmErrors::DecodeFailed(PROVIDER, e.to_string())
    })
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// History messages in OpenAI's shape. See the module docs for the `tool_call_id` rule:
/// `pending_tool_ids` is the per-assistant-message queue, consumed in order by the `tool` messages
/// that follow it, exactly mirroring how the agent itself matches them.
fn openai_messages(messages: &[ChatMessage]) -> Vec<Value> {
    let mut out = Vec::with_capacity(messages.len());
    let mut pending_tool_ids: VecDeque<String> = VecDeque::new();

    for (idx, message) in messages.iter().enumerate() {
        if message.role == "tool" {
            let tool_call_id = pending_tool_ids.pop_front().unwrap_or_else(|| format!("call_unmatched_{idx}"));
            out.push(json!({"role": "tool", "tool_call_id": tool_call_id, "content": message.content}));
            continue;
        }

        let content = match message.images.as_deref().filter(|images| !images.is_empty()) {
            Some(images) => {
                let mut parts: Vec<Value> = Vec::new();
                if !message.content.is_empty() {
                    parts.push(json!({"type": "text", "text": message.content}));
                }
                for b64 in images {
                    parts.push(json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{b64}")}}));
                }
                Value::Array(parts)
            }
            None => Value::String(message.content.clone()),
        };
        let mut out_message = json!({"role": message.role, "content": content});

        if let Some(calls) = message.tool_calls.as_deref().filter(|calls| !calls.is_empty()) {
            pending_tool_ids.clear();
            let synthesized: Vec<Value> = calls
                .iter()
                .enumerate()
                .map(|(i, call)| {
                    let id = format!("call_{idx}_{i}");
                    pending_tool_ids.push_back(id.clone());
                    json!({
                        "id": id,
                        "type": "function",
                        "function": {"name": call.function.name, "arguments": call.function.arguments.to_string()},
                    })
                })
                .collect();
            out_message["tool_calls"] = Value::Array(synthesized);
        }
        out.push(out_message);
    }
    out
}

/// The request body. `think` mirrors `ThinkChoice`: `Enabled(false)` disables thinking, `Enabled(true)`
/// (or none) enables it at the model's own default effort, and a level is sent exactly as given —
/// never rewritten to a different value.
fn build_payload(
    model: &str,
    messages: Vec<Value>,
    think: Option<ThinkChoice>,
    num_predict: i32,
    tools: Option<&[ToolDefinition]>,
    sampling: &Sampling,
) -> Value {
    let mut kwargs = serde_json::Map::new();
    match think {
        None | Some(ThinkChoice::Enabled(true)) => {
            kwargs.insert("enable_thinking".into(), json!(true));
        }
        Some(ThinkChoice::Enabled(false)) => {
            kwargs.insert("enable_thinking".into(), json!(false));
        }
        Some(ThinkChoice::Level(level)) => {
            kwargs.insert("enable_thinking".into(), json!(true));
            kwargs.insert("reasoning_effort".into(), json!(level));
        }
    }

    let mut payload = json!({
        "model": model,
        "stream": false,
        "messages": messages,
        "chat_template_kwargs": Value::Object(kwargs),
        "max_tokens": num_predict,
    });
    if let Some(tools) = tools.filter(|tools| !tools.is_empty()) {
        payload["tools"] = serde_json::to_value(tools).unwrap_or(Value::Null);
    }
    // Sampling is sent only when set: a missing value leaves the server's own default in force.
    let optional = [
        ("temperature", sampling.temperature.map(|v| json!(v))),
        ("top_p", sampling.top_p.map(|v| json!(v))),
        ("top_k", sampling.top_k.map(|v| json!(v))),
        ("min_p", sampling.min_p.map(|v| json!(v))),
        ("repeat_penalty", sampling.repeat_penalty.map(|v| json!(v))),
        ("presence_penalty", sampling.presence_penalty.map(|v| json!(v))),
        ("seed", sampling.seed.map(|v| json!(v))),
    ];
    for (key, value) in optional {
        if let Some(value) = value {
            payload[key] = value;
        }
    }
    payload
}

#[derive(Deserialize, Default)]
struct CompletionResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    timings: Option<Timings>,
}

#[derive(Deserialize, Default)]
struct Choice {
    #[serde(default)]
    message: WireMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct WireMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<WireToolCall>>,
}

#[derive(Deserialize)]
struct WireToolCall {
    #[serde(default)]
    id: Option<String>,
    function: WireFunction,
}

#[derive(Deserialize)]
struct WireFunction {
    #[serde(default)]
    name: String,
    /// A JSON *string* on the wire, so it is parsed back into a value (see the module docs).
    #[serde(default)]
    arguments: Option<Value>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: Option<u64>,
    #[serde(default)]
    completion_tokens: Option<u64>,
}

#[derive(Deserialize, Default)]
struct Timings {
    #[serde(default)]
    prompt_ms: Option<f64>,
    #[serde(default)]
    predicted_ms: Option<f64>,
    /// How many prompt tokens the server actually evaluated: the rest came from its cache.
    #[serde(default)]
    prompt_n: Option<u64>,
}

/// A tool call's arguments arrive as a JSON string; a value that doesn't parse is passed through
/// as the raw string rather than guessed at, so the tool's own argument parsing fails loudly with a
/// real error instead of the call silently doing something else.
fn parse_tool_call_arguments(raw: Option<Value>) -> Value {
    match raw {
        Some(Value::String(text)) => serde_json::from_str(&text).unwrap_or(Value::String(text)),
        Some(other) => other,
        None => json!({}),
    }
}

impl CompletionResponse {
    fn into_chat_response(self, model: &str) -> ChatResponse {
        let metrics = self.metrics();
        let choice = self.choices.into_iter().next().unwrap_or_default();
        let message = choice.message;

        let tool_calls = message.tool_calls.map(|calls| {
            calls
                .into_iter()
                .enumerate()
                .map(|(i, call)| ModelToolCall {
                    id: call.id.unwrap_or_else(|| format!("call_{i}")),
                    function: ModelToolCallFunction {
                        index: None,
                        name: call.function.name,
                        arguments: parse_tool_call_arguments(call.function.arguments),
                    },
                })
                .collect::<Vec<_>>()
        });

        // Some replies leave the whole `<think>...</think>` block in `content`: split it out, with
        // the server's own `reasoning_content` winning when it gave one.
        let (thinking, content) =
            ensure_thinking_split(message.reasoning_content.filter(|t| !t.is_empty()), message.content.unwrap_or_default());
        let message = ChatMessage {
            role: "assistant".to_string(),
            content,
            tool_calls: tool_calls.filter(|calls| !calls.is_empty()),
            tool_name: None,
            thinking,
            images: None,
        };
        ChatResponse::new(model.to_string(), now_rfc3339(), message, choice.finish_reason, metrics)
    }

    fn metrics(&self) -> ResponseMetrics {
        let usage = self.usage.as_ref();
        let timings = self.timings.as_ref();
        let nanos = |ms: Option<f64>| ms.map(|ms| (ms * 1e6) as u64);
        ResponseMetrics {
            // llama-server doesn't report a load time: the model is loaded before it answers at all.
            load_duration: Some(0),
            prompt_eval_count: usage.and_then(|u| u.prompt_tokens),
            prompt_eval_duration: Some(nanos(timings.and_then(|t| t.prompt_ms)).unwrap_or(0)),
            prompt_eval_processed: timings.and_then(|t| t.prompt_n),
            eval_count: usage.and_then(|u| u.completion_tokens),
            eval_duration: Some(nanos(timings.and_then(|t| t.predicted_ms)).unwrap_or(0)),
        }
    }
}

/// Logs the per-request numbers llama-server reports, plus the generation speed they imply.
fn log_metrics(response: &CompletionResponse) {
    let m = response.metrics();
    let tokens_per_second = match (m.eval_count, m.eval_duration) {
        (Some(count), Some(duration_ns)) if duration_ns > 0 => Some(count as f64 / (duration_ns as f64 / 1e9)),
        _ => None,
    };
    tracing::info!(
        prompt_eval_count = m.prompt_eval_count,
        prompt_eval_processed = m.prompt_eval_processed,
        prompt_eval_duration_s = m.prompt_eval_duration.map(|ns| ns as f64 / 1e9),
        eval_count = m.eval_count,
        eval_duration_s = m.eval_duration.map(|ns| ns as f64 / 1e9),
        tokens_per_second,
        "llama-server call finished"
    );
}

#[derive(Deserialize, Default)]
struct Props {
    #[serde(default)]
    chat_template: String,
    #[serde(default)]
    model_path: Option<String>,
    #[serde(default)]
    model_ftype: Option<String>,
    #[serde(default)]
    total_slots: Option<u64>,
    #[serde(default)]
    default_generation_settings: Option<GenerationSettings>,
}

#[derive(Deserialize, Default)]
struct GenerationSettings {
    #[serde(default)]
    n_ctx: Option<u64>,
}

impl Props {
    /// The loaded model's file name, without its folders.
    fn model_file(&self) -> Option<String> {
        let path = self.model_path.as_deref()?;
        path.rsplit(['/', '\\']).next().filter(|name| !name.is_empty()).map(String::from)
    }
}

/// The plain `name value` lines of llama-server's `/metrics` (comments and labelled series skipped)
/// as a map. The gauges that matter here have no labels.
fn prometheus_gauges(text: &str) -> std::collections::HashMap<String, f64> {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.contains('{'))
        .filter_map(|line| {
            let (name, value) = line.split_once(' ')?;
            Some((name.to_string(), value.trim().parse::<f64>().ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into(), tool_calls: None, tool_name: None, thinking: None, images: None }
    }

    fn assistant_calling(names: &[&str]) -> ChatMessage {
        ChatMessage {
            tool_calls: Some(
                names
                    .iter()
                    .map(|name| ModelToolCall {
                        id: String::new(),
                        function: ModelToolCallFunction { index: None, name: name.to_string(), arguments: json!({"city": "Paris"}) },
                    })
                    .collect(),
            ),
            ..message("assistant", "")
        }
    }

    #[test]
    fn tool_results_are_matched_to_their_calls_by_order() {
        let history = vec![
            message("user", "weather?"),
            assistant_calling(&["get_temperature", "get_wind"]),
            ChatMessage { tool_name: Some("get_temperature".into()), ..message("tool", "21") },
            ChatMessage { tool_name: Some("get_wind".into()), ..message("tool", "5") },
            message("assistant", "21 degrees, light wind"),
        ];
        let out = openai_messages(&history);

        assert_eq!(out[1]["tool_calls"][0]["id"], "call_1_0");
        assert_eq!(out[1]["tool_calls"][1]["id"], "call_1_1");
        // Arguments go out as the JSON string OpenAI expects
        assert_eq!(out[1]["tool_calls"][0]["function"]["arguments"], "{\"city\":\"Paris\"}");
        assert_eq!(out[2]["tool_call_id"], "call_1_0");
        assert_eq!(out[3]["tool_call_id"], "call_1_1");
        // A tool message with no call left to answer still gets an id, so the request stays valid
        let orphan = openai_messages(&[message("tool", "stray")]);
        assert_eq!(orphan[0]["tool_call_id"], "call_unmatched_0");
    }

    #[test]
    fn images_become_data_url_parts_and_text_stays_first() {
        let with_image = ChatMessage { images: Some(vec!["AAAA".into()]), ..message("user", "what is this?") };
        let out = openai_messages(&[with_image]);
        assert_eq!(out[0]["content"][0], json!({"type": "text", "text": "what is this?"}));
        assert_eq!(out[0]["content"][1]["image_url"]["url"], "data:image/png;base64,AAAA");

        let image_only = ChatMessage { images: Some(vec!["AAAA".into()]), ..message("user", "") };
        assert_eq!(openai_messages(&[image_only])[0]["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn think_choices_map_to_template_kwargs() {
        let kwargs = |think| build_payload("m", vec![], think, 10, None, &Sampling::default())["chat_template_kwargs"].clone();
        assert_eq!(kwargs(None), json!({"enable_thinking": true}));
        assert_eq!(kwargs(Some(ThinkChoice::Enabled(true))), json!({"enable_thinking": true}));
        assert_eq!(kwargs(Some(ThinkChoice::Enabled(false))), json!({"enable_thinking": false}));
        assert_eq!(
            kwargs(Some(ThinkChoice::Level("low".into()))),
            json!({"enable_thinking": true, "reasoning_effort": "low"})
        );
    }

    #[test]
    fn sampling_is_sent_only_when_set() {
        let none = build_payload("m", vec![], None, 10, None, &Sampling::default());
        for key in ["temperature", "top_p", "top_k", "min_p", "repeat_penalty", "presence_penalty", "seed"] {
            assert!(none.get(key).is_none(), "{key} must be absent when unset");
        }
        assert_eq!(none["max_tokens"], 10);

        let sampling = Sampling { temperature: Some(0.6), top_k: Some(20), seed: Some(7), ..Default::default() };
        let set = build_payload("m", vec![], None, 10, None, &sampling);
        assert_eq!(set["top_k"], 20);
        assert_eq!(set["seed"], 7);
        assert!((set["temperature"].as_f64().unwrap() - 0.6).abs() < 1e-6);
        assert!(set.get("top_p").is_none());
    }

    #[test]
    fn a_reply_with_a_tool_call_and_reasoning_is_converted() {
        let body = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "reasoning_content": "need the temperature",
                    "tool_calls": [{"id": "abc", "type": "function",
                                    "function": {"name": "get_temperature", "arguments": "{\"location\":\"Paris\"}"}}]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 120, "completion_tokens": 7},
            "timings": {"prompt_ms": 12.5, "predicted_ms": 70.0, "prompt_n": 20}
        }"#;
        let response = serde_json::from_str::<CompletionResponse>(body).unwrap().into_chat_response("m");

        assert_eq!(response.message.thinking.as_deref(), Some("need the temperature"));
        assert_eq!(response.done_reason.as_deref(), Some("tool_calls"));
        let calls = response.message.tool_calls.as_ref().unwrap();
        assert_eq!(calls[0].id, "abc");
        assert_eq!(calls[0].function.arguments, json!({"location": "Paris"}));
        assert_eq!(response.prompt_eval_count(), Some(120));
        assert_eq!(response.eval_count(), Some(7));
        assert_eq!(response.prompt_processed_tokens(), Some(20));
        assert_eq!(response.eval_duration_ms(), Some(70));
        assert_eq!(response.prompt_eval_duration_ms(), Some(12));
    }

    #[test]
    fn malformed_tool_arguments_are_passed_through_not_guessed() {
        assert_eq!(parse_tool_call_arguments(Some(json!("{not json"))), json!("{not json"));
        assert_eq!(parse_tool_call_arguments(None), json!({}));
        assert_eq!(parse_tool_call_arguments(Some(json!("{\"a\":1}"))), json!({"a": 1}));
    }

    #[test]
    fn an_unclosed_think_block_in_content_is_split_out() {
        let body = r#"{"choices":[{"message":{"content":"musing</think>the answer"},"finish_reason":"stop"}]}"#;
        let response = serde_json::from_str::<CompletionResponse>(body).unwrap().into_chat_response("m");
        assert_eq!(response.message.thinking.as_deref(), Some("musing"));
        assert_eq!(response.message.content, "the answer");
    }

    #[test]
    fn prometheus_gauges_skip_comments_and_labelled_series() {
        let text = "# HELP x\nllamacpp:prompt_tokens_total 120\nllamacpp:thing{slot=\"0\"} 4\nllamacpp:tokens_predicted_total 7.5\n";
        let gauges = prometheus_gauges(text);
        assert_eq!(gauges.len(), 2);
        assert_eq!(gauges["llamacpp:prompt_tokens_total"], 120.0);
    }

    #[test]
    fn the_model_file_is_the_last_path_component() {
        let props = Props { model_path: Some("/models/sub/Qwen3.6-35B.gguf".into()), ..Default::default() };
        assert_eq!(props.model_file().as_deref(), Some("Qwen3.6-35B.gguf"));
        assert_eq!(Props::default().model_file(), None);
    }
}
