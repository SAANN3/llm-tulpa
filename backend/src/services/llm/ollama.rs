//! Ollama as a model provider: the chat, generate, template, tag and process calls against its HTTP
//! API, converted to and from the shared types in `types.rs`, plus the model-management calls
//! (pull, import) in `transfer`.

mod transfer;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

pub use transfer::ImportProgress;

use super::budget::OutputBudget;
use super::prefix_watch::PrefixWatch;
use super::provider::LlmProvider;
use super::tool_defs::{tool_definitions, ToolDefinition};
use super::template::{ensure_thinking_split, extract_tool_call_markers, parse_thinking_capability};
use super::types::{
    CallParams, ChatMessage, ChatResponse, GenerateResponse, LaunchRequest, LlmErrors, LocalModel, ResponseMetrics, RunningModels, Sampling,
    ThinkChoice, ThinkingCapability,
};
use crate::services::error::ErrorService;
use crate::services::llama_runtime::{CallGuard, LlamaRuntime};
use crate::tools::base::Tool;

/// The name this provider is registered under (`llm_providers.name`) and labels its errors with.
pub const PROVIDER: &str = "ollama";

/// Talks to Ollama's HTTP API and parses its responses into our own structs. Route
/// handlers go through this rather than calling Ollama directly, so the rest of the
/// app never has to know Ollama's wire format.
pub struct OllamaService {
    client: reqwest::Client,
    /// Where Ollama answers; the owner can change it while the backend runs (see `set_base`)
    base_url: std::sync::RwLock<String>,
    /// The model's context window (`ollama.context_length` in `settings.json`) — the ceiling
    /// each request's `num_predict` cap is computed under (see `OutputBudget`). Ollama defaults
    /// `num_predict` to unlimited when it's omitted, so without a cap a model that never emits a
    /// stop token keeps generating indefinitely.
    budget: OutputBudget,
    /// Reports a request that rewrites the previous one's prefix (see `PrefixWatch`).
    prefix_watch: PrefixWatch,
    /// The managed llama.cpp, which an Ollama turn asks to free the GPU first (see `acquire`).
    llama: std::sync::OnceLock<Arc<LlamaRuntime>>,
}

impl OllamaService {
    /// No model is held here — every call names the one it runs against (a chat's bound
    /// model, or the user's active one), resolved from the database by the caller.
    pub fn new(base_url: impl Into<String>, max_predict_tokens: i32) -> Self {
        let budget = OutputBudget::new(max_predict_tokens.max(0) as u64);
        Self {
            client: reqwest::Client::builder()
                .timeout(budget.default_timeout())
                .build()
                .expect("failed to build ollama http client"),
            base_url: std::sync::RwLock::new(base_url.into()),
            budget,
            prefix_watch: PrefixWatch::default(),
            llama: std::sync::OnceLock::new(),
        }
    }

    pub fn base(&self) -> String {
        self.base_url.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Points the client somewhere else from the next request on.
    pub fn set_base(&self, url: String) {
        *self.base_url.write().unwrap_or_else(|p| p.into_inner()) = url;
    }

    /// The address as it is stored: trimmed, without a trailing slash, an http(s) URL with a host.
    pub fn normalize_url(url: &str) -> Result<String, ErrorService> {
        let trimmed = url.trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(trimmed).map_err(|_| ErrorService::new(axum::http::StatusCode::BAD_REQUEST, "that is not a web address (try http://localhost:11434)"))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() || parsed.path() != "/" && !parsed.path().is_empty() || parsed.query().is_some() {
            return Err(ErrorService::new(axum::http::StatusCode::BAD_REQUEST, "the address must be an http or https address with a host and port only, like http://localhost:11434"));
        }
        Ok(trimmed.to_string())
    }

    /// How many models the Ollama at `url` has installed, or why it can't be reached. A fresh,
    /// short-lived request: nothing of this client's own is changed.
    pub async fn probe(url: &str) -> Result<usize, String> {
        let res = reqwest::Client::new()
            .get(format!("{url}/api/tags"))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| format!("nothing answers there ({})", e.without_url()))?;
        if !res.status().is_success() {
            return Err(format!("it answered with status {}", res.status()));
        }
        #[derive(Deserialize)]
        struct Tags {
            #[serde(default)]
            models: Vec<Value>,
        }
        res.json::<Tags>().await.map(|t| t.models.len()).map_err(|_| "that is not an Ollama (its answer isn't what Ollama sends)".to_string())
    }

    /// Tells this client about the managed llama.cpp it shares the GPU with.
    pub fn share_gpu_with(&self, runtime: Arc<LlamaRuntime>) {
        let _ = self.llama.set(runtime);
    }

    /// Asks Ollama to drop every model it has loaded, so the GPU's memory is free for llama.cpp.
    /// Best effort and quick: no Ollama running, or one that doesn't answer, is the normal case for
    /// someone who only uses llama.cpp.
    pub async fn unload_all(&self) {
        let probe = Duration::from_secs(2);
        let Ok(res) = self.client.get(format!("{}/api/ps", self.base())).timeout(probe).send().await else { return };
        let Ok(running) = res.json::<RunningModels>().await else { return };
        for model in running.models {
            tracing::info!("unloading Ollama's {} to free the GPU for llama.cpp", model.name);
            let body = serde_json::json!({"model": model.name, "keep_alive": 0});
            let _ = self.client.post(format!("{}/api/generate", self.base())).timeout(Duration::from_secs(30)).json(&body).send().await;
        }
    }
    /// The given `model`'s raw Jinja chat template, straight from Ollama's `/api/show`
    /// — see `thinking_capability` for why that's the real template and why it's read
    /// fresh every time.
    async fn chat_template(&self, model: &str) -> Result<String, LlmErrors> {
        let url = format!("{}/api/show", self.base());
        let body = serde_json::json!({ "model": model });

        let res = self.client.post(&url).json(&body).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/show request failed");
            LlmErrors::RequestFailed(PROVIDER, e.to_string())
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/show returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }

        #[derive(Deserialize, Default)]
        struct ShowResponse {
            #[serde(default)]
            template: String,
        }
        let parsed: ShowResponse = decode_response(res).await?;
        Ok(parsed.template)
    }

    /// Fetches ollama.com's public model library page. There is no API behind it, so
    /// `ModelLibrary` parses the returned HTML into a structured catalog.
    pub async fn fetch_catalog(&self) -> Result<String, LlmErrors> {
        let res = self
            .client
            .get("https://ollama.com/library")
            .header("Accept", "text/html")
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "ollama.com/library request failed");
                LlmErrors::RequestFailed(PROVIDER, e.to_string())
            })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama.com/library returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }

        res.text().await.map_err(|e| LlmErrors::DecodeFailed(PROVIDER, e.to_string()))
    }
}

#[async_trait]
impl LlmProvider for OllamaService {
    fn name(&self) -> &'static str {
        PROVIDER
    }

    /// Calls Ollama's legacy `/api/generate` endpoint: raw prompt string in, raw
    /// completion string out. No message roles, no chat history, no tool calls — kept
    /// around as the simplest possible path to a model response. `chat` below is the
    /// one actually meant for multi-turn/tool-calling use. `think` asks the model to
    /// reason before answering and defaults to `true` when not given — plain
    /// bool only, no effort-level choice, since every caller of this endpoint is an
    /// internal, non-user-facing call (chat naming, compaction summaries) with no UI
    /// behind it to pick a level from; see `split_thinking` — this endpoint's
    /// `thinking` response field isn't populated for this model, so the reasoning
    /// trace comes back embedded in `response` instead and has to be pulled back out
    /// on our end.
    async fn generate(
        &self,
        prompt: String,
        think: Option<bool>,
        model: &str,
        params: &CallParams,
    ) -> Result<GenerateResponse, LlmErrors> {
        let url = format!("{}/api/generate", self.base());
        let num_predict = self.budget.for_generate(&prompt, params.context_length);
        let body = OllamaGenerateRequest {
            model: model.to_string(),
            prompt,
            stream: false,
            think: Value::Bool(think.unwrap_or(true)),
            options: OllamaOptions::new(num_predict, &params.sampling),
        };

        tracing::info!("calling ollama /api/generate");
        let res = self.client.post(&url).json(&body).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/generate request failed");
            LlmErrors::RequestFailed(PROVIDER, e.to_string())
        })?;

        // reqwest's `send` only errors on transport-level failures (connection refused,
        // timeout, TLS) — an HTTP error status like 500 still comes back as `Ok`. Without
        // this check, a non-2xx response (whose body likely isn't our expected JSON shape)
        // would surface as a confusing decode error instead of the actual status.
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/generate returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }

        // `/api/generate` never populates `thinking` for this model (see the doc comment
        // above) — `ensure_thinking_split` always has something to do here.
        let mut result: OllamaGenerateWire = decode_response(res).await?;
        log_ollama_metrics(&result.metrics);
        let (thinking, response) = ensure_thinking_split(result.thinking.take(), result.response);
        Ok(GenerateResponse { model: result.model, created_at: result.created_at, response, thinking })
    }

    /// Calls Ollama's `/api/chat` endpoint. `messages` is the conversation to send, in
    /// order. `new_message`, when given, is appended after `messages` as the turn being
    /// added now — kept as a separate parameter rather than requiring callers to fold it
    /// into `messages` themselves, since not every call has a new turn to add (a caller
    /// asking the model to continue based on the existing history alone passes `None`).
    /// `tools` is whatever the caller wants advertised to the model for this call, mapped
    /// via `tool_definitions` and omitted entirely if empty. Assembling and executing on
    /// a `tool_calls` response is the caller's job. `think` asks the model to reason
    /// before answering and defaults to enabled (no specific effort level — Ollama's
    /// own default) when not given — see `think_param` for exactly what gets sent on
    /// the wire for each `ThinkChoice` variant.
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
        let url = format!("{}/api/chat", self.base());

        let definitions = tool_definitions(tools);

        // Serialize the tool definitions to compact JSON (exactly what hits Ollama's wire)
        // and count the bytes → tokens for an accurate overhead estimate, computed before
        // `definitions` moves into `tools` below so nothing needs cloning for this.
        let tool_overhead_tokens = if definitions.is_empty() {
            0
        } else {
            let json_bytes = serde_json::to_string(&definitions).unwrap_or_default().len();
            OutputBudget::tool_overhead_tokens(json_bytes)
        };

        let tools = if definitions.is_empty() {
            None
        } else {
            Some(definitions)
        };

        let mut messages = messages;
        if let Some(new_message) = new_message {
            messages.push(new_message);
        }

        let num_predict = self.budget.for_chat(&messages, tool_overhead_tokens, known_prompt_tokens, params.context_length);
        self.prefix_watch.check(model, &tools, &messages);
        let body = OllamaChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
            think: think_param(think),
            tools,
            options: OllamaOptions::new(num_predict, &params.sampling),
        };

        tracing::info!("calling ollama /api/chat");
        let res = self.client.post(&url).json(&body).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/chat request failed");
            LlmErrors::RequestFailed(PROVIDER, e.to_string())
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/chat returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }

        // `/api/chat` usually populates `message.thinking` correctly on its own (unlike
        // `/api/generate` — see its doc comment) and leaves `content` clean — but not
        // reliably: at least one observed response came back with `thinking` non-empty
        // while `content` *still* held the whole raw `<think>...</think>` block,
        // unsplit. `ensure_thinking_split` triggers on that marker actually being
        // present in `content`, not on whether `thinking` looks unset, so this gets
        // caught too.
        let mut result: OllamaChatWire = decode_response(res).await?;
        log_ollama_metrics(&result.metrics);
        let (thinking, content) = ensure_thinking_split(result.message.thinking.take(), result.message.content);
        result.message.thinking = thinking;
        result.message.content = content;
        Ok(result.into())
    }

    /// What the given `model` actually supports for `think`, discovered
    /// live from its own chat template rather than hardcoded for one specific model —
    /// deliberately not cached/persisted anywhere: this calls out fresh every time a
    /// caller asks (cheap — Ollama's `/api/show` reads stored model metadata, no load
    /// required), so it's automatically correct the moment the active model changes,
    /// with nothing to invalidate. Calls Ollama's `/api/show` (not `/api/chat`/
    /// `/api/generate`) specifically because it returns the model's raw `template`
    /// text — confirmed live that this is the *actual* Jinja chat template embedded in
    /// the GGUF (byte-for-byte the same content `llama-mtp`'s own `/props` returns for
    /// the same model), not some Ollama-internal abstraction of it, despite
    /// `OLLAMA_GO_TEMPLATE` appearing in its own startup config. This is also exactly
    /// why a llama-server needs its own translation of `/api/show` (from `/props`) —
    /// this method works unchanged against either backend.
    async fn thinking_capability(&self, model: &str) -> Result<ThinkingCapability, LlmErrors> {
        Ok(parse_thinking_capability(&self.chat_template(model).await?))
    }

    /// The tags the given `model`'s own chat template wraps a tool call in (e.g.
    /// `<tool_call>` and `</tool_call>`), discovered live the same way as
    /// `thinking_capability` — see `extract_tool_call_markers`. Empty if the template
    /// has none or can't be read, in which case nothing that looks for them can
    /// trigger: a failure to look is never treated as a finding.
    async fn tool_call_markers(&self, model: &str) -> Vec<String> {
        match self.chat_template(model).await {
            Ok(template) => extract_tool_call_markers(&template),
            Err(_) => vec![],
        }
    }

    /// The models actually installed in this Ollama instance, from `/api/tags` — the
    /// authoritative "what can I switch to right now without pulling" list. Ollama's own
    /// fields (`details.parameter_size`/`quantization_level`) are passed straight through
    /// so the client can show/estimate requirements without a second round trip.
    async fn list_local_models(&self) -> Result<Vec<LocalModel>, LlmErrors> {
        let url = format!("{}/api/tags", self.base());

        let res = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/tags request failed");
            LlmErrors::RequestFailed(PROVIDER, e.to_string())
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/tags returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }

        #[derive(Deserialize, Default)]
        struct TagsResponse {
            #[serde(default)]
            models: Vec<LocalModel>,
        }
        let parsed: TagsResponse = decode_response(res).await?;
        Ok(parsed.models)
    }

    /// The models the backend has loaded right now, from `/api/ps` — answered by Ollama itself, or
    /// Empty when nothing is loaded.
    /// Before an Ollama turn: llama.cpp gives the GPU back if nobody is in the middle of a turn on it,
    /// so the two never hold memory at once. If someone is, Ollama goes ahead and may spill to the CPU.
    async fn acquire(&self, launch: Option<&LaunchRequest>) -> Result<CallGuard, ErrorService> {
        let _ = launch;
        if let Some(runtime) = self.llama.get() {
            runtime.release_gpu().await;
        }
        Ok(CallGuard::none())
    }

    async fn running_models(&self) -> Result<RunningModels, LlmErrors> {
        let url = format!("{}/api/ps", self.base());

        let res = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/ps request failed");
            LlmErrors::RequestFailed(PROVIDER, e.to_string())
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/ps returned a non-success status");
            return Err(LlmErrors::UnexpectedStatus(PROVIDER, status, body));
        }

        decode_response(res).await
    }
}

/// Reads the response body as text before parsing it, rather than `res.json()`
/// directly, purely so a parse failure can still log what Ollama actually sent — a bare
/// `serde_json`/reqwest decode error on its own doesn't say what the body looked like,
/// which makes a schema drift between us and Ollama's actual API hard to diagnose from
/// the error message alone.
async fn decode_response<T: DeserializeOwned>(res: reqwest::Response) -> Result<T, LlmErrors> {
    let body_text = res
        .text()
        .await
        .map_err(|e| LlmErrors::DecodeFailed(PROVIDER, e.to_string()))?;

    serde_json::from_str(&body_text).map_err(|e| {
        tracing::error!("failed to decode ollama response: {e}\nbody = {body_text}");
        LlmErrors::DecodeFailed(PROVIDER, e.to_string())
    })
}

/// What the wire-level `think` field actually sends for a given `ThinkChoice`.
/// Confirmed directly against the running Ollama binary (`grep -a` over
/// `/bin/ollama` turns up the literal strings `reasoning_effort` and `"low",
/// "medium", "high", "xhigh"`) that a graded string here is genuine, native
/// behavior, not a guess — Ollama forwards it straight through to the model's own
/// chat template, same mechanism `llama-mtp`'s `chat_template_kwargs.reasoning_effort`
/// uses. No default level is forced here — `None`/`Enabled(true)` sends plain `true`,
/// matching this project's original behavior exactly (a hardcoded default level was
/// tried and deliberately reverted in favor of this real, discoverable, user-chosen
/// one — see project memory).
fn think_param(think: Option<ThinkChoice>) -> Value {
    match think {
        None | Some(ThinkChoice::Enabled(true)) => Value::Bool(true),
        Some(ThinkChoice::Enabled(false)) => Value::Bool(false),
        Some(ThinkChoice::Level(level)) => Value::String(level),
    }
}

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    think: Value,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    think: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolDefinition>>,
    options: OllamaOptions,
}

/// Generation options shared by both `/api/generate` and `/api/chat` requests. See
/// `OllamaService::budget`. Sampling values are only sent when set: a missing one leaves the model's
/// own default in force.
#[derive(Serialize)]
struct OllamaOptions {
    num_predict: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
}

impl OllamaOptions {
    fn new(num_predict: i32, sampling: &Sampling) -> Self {
        Self {
            num_predict,
            temperature: sampling.temperature,
            top_p: sampling.top_p,
            top_k: sampling.top_k,
            min_p: sampling.min_p,
            repeat_penalty: sampling.repeat_penalty,
            presence_penalty: sampling.presence_penalty,
            seed: sampling.seed,
        }
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
    /// Not an Ollama field: llama.cpp reports it, because llama-server's prompt timing covers only
    /// the tokens it evaluated while `prompt_eval_count` is the whole prompt (cached part
    /// included), and a speed needs the tokens that the time was spent on.
    #[serde(default)]
    prompt_eval_processed: Option<u64>,
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

/// Mirrors Ollama's `/api/generate` response shape, not our own API contract — `generate` converts
/// it to the shared `GenerateResponse`, so Ollama's shape never leaks to callers.
#[derive(Deserialize)]
struct OllamaGenerateWire {
    model: String,
    created_at: String,
    response: String,
    /// Populated by `generate` after decoding, from `split_thinking` — not something Ollama's own
    /// JSON reliably fills in for this model (see `split_thinking`'s docs).
    #[serde(default)]
    thinking: Option<String>,
    #[serde(flatten)]
    metrics: OllamaMetrics,
}

/// Mirrors Ollama's `/api/chat` response shape — see `OllamaGenerateWire` for why this isn't
/// reused as-is.
#[derive(Deserialize)]
struct OllamaChatWire {
    model: String,
    created_at: String,
    message: ChatMessage,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(flatten)]
    metrics: OllamaMetrics,
}

impl From<OllamaMetrics> for ResponseMetrics {
    fn from(m: OllamaMetrics) -> Self {
        Self {
            load_duration: m.load_duration,
            prompt_eval_count: m.prompt_eval_count,
            prompt_eval_duration: m.prompt_eval_duration,
            prompt_eval_processed: m.prompt_eval_processed,
            eval_count: m.eval_count,
            eval_duration: m.eval_duration,
        }
    }
}

impl From<OllamaChatWire> for ChatResponse {
    fn from(wire: OllamaChatWire) -> Self {
        ChatResponse::new(wire.model, wire.created_at, wire.message, wire.done_reason, wire.metrics.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_checked_and_tidied() {
        let ok = |url: &str| OllamaService::normalize_url(url).ok();
        assert_eq!(ok(" http://localhost:11434/ "), Some("http://localhost:11434".to_string()));
        assert_eq!(ok("https://ollama.example.org"), Some("https://ollama.example.org".to_string()));
        assert_eq!(ok("localhost:11434"), None, "no scheme");
        assert_eq!(ok("ftp://localhost:11434"), None);
        assert_eq!(ok("http://localhost:11434/api/tags"), None, "a path isn't an address");
        assert_eq!(ok("http://"), None);
    }

    #[test]
    fn test_ollama_chat_response_deserialization() {
        let json_data = r#"{
            "model": "qwen2.5:32b",
            "created_at": "2026-09-24T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Done."
            },
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 67139,
            "eval_count": 254
        }"#;

        let wire: OllamaChatWire = serde_json::from_str(json_data).unwrap();
        let resp: ChatResponse = wire.into();
        assert_eq!(resp.prompt_eval_count(), Some(67139));
        assert_eq!(resp.eval_count(), Some(254));
        assert_eq!(resp.done_reason.as_deref(), Some("stop"));
    }
}
