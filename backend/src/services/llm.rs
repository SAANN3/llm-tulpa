mod transfer;

use std::collections::BTreeMap;
use std::time::Duration;

use axum::http::StatusCode;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

pub use transfer::ImportProgress;

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

/// How many characters per token for a rough prompt-size estimate used to cap
/// `num_predict` per-request — applied before subtracting from `max_predict_tokens` to
/// leave room for whatever the model generates. Should sit slightly *below* the real
/// ratio of this project's actual traffic, not far from it in either direction: agentic
/// tool/code-heavy content measured ~2.5 chars/token over the counted message text on a
/// real 48k-token chat (English prose alone would be ~3.5–4). Too high undercounts the
/// prompt, so a request already near the real context ceiling gets a `num_predict`
/// larger than the room actually left and is hard-truncated by llama.cpp mid-output —
/// including mid-tool-call-JSON, which fails the whole turn. Too low overcounts, which
/// is worse in practice: the estimate exceeds the whole window on an ordinary
/// mid-size prompt, `num_predict` collapses to its floor, and every reply gets cut off
/// (thinking alone can consume a small budget) on every turn, not just near the ceiling.
const PROMPT_CHARS_PER_TOKEN: f64 = 2.5;

/// Lowest `num_predict` the per-request calculation will ever return. Has to be big
/// enough for a reply with thinking enabled to actually finish — a budget in the low
/// hundreds is spent entirely inside the reasoning trace, leaving an empty reply. If
/// the real room left is smaller than this the request is near the context ceiling
/// regardless, and compaction (not this cap) is what's meant to prevent that.
const MIN_NUM_PREDICT: u64 = 2048;

/// A flat token overhead added to the prompt-size estimate for content that character
/// count alone can't capture — tool definition JSON schemas, chat template special
/// tokens (`<s>`, `<|start_header|>`, etc.), and per-token overhead from the tokenizer
/// itself. Chosen to comfortably cover a typical tool-calling workload (6–8 tools with
/// full parameter schemas ≈ 3–5K tokens) with room for heavier setups; also doubles
/// for the `generate` path where it covers template overhead without tool definitions.
const PROMPT_TOKEN_SAFETY_MARGIN: u64 = 1024;

fn request_timeout(max_predict_tokens: i32) -> Duration {
    Duration::from_secs_f64(max_predict_tokens as f64 / MIN_TOKENS_PER_SEC) + PROMPT_PROCESSING_BUFFER
}

/// Talks to Ollama's HTTP API and parses its responses into our own structs. Route
/// handlers go through this rather than calling Ollama directly, so the rest of the
/// app never has to know Ollama's wire format.
pub struct OllamaService {
    client: reqwest::Client,
    base_url: String,
    /// The model's context window (`ollama.context_length` in `settings.json`) — the ceiling
    /// each request's `num_predict` cap is computed under (see `compute_num_predict_base`).
    /// Ollama defaults `num_predict` to unlimited when it's omitted, so without a cap a model
    /// that never emits a stop token keeps generating indefinitely.
    max_predict_tokens: i32,
}

impl OllamaService {
    /// No model is held here — every call names the one it runs against (a chat's bound
    /// model, or the user's active one), resolved from the database by the caller.
    pub fn new(base_url: impl Into<String>, max_predict_tokens: i32) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(request_timeout(max_predict_tokens))
                .build()
                .expect("failed to build ollama http client"),
            base_url: base_url.into(),
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

    /// Computes a per-request `num_predict` cap for the `generate` path: `char_count`
    /// is just `prompt`'s own length, no tool overhead — see `compute_num_predict_base`.
    fn compute_num_predict_generate(&self, prompt: &str) -> i32 {
        self.compute_num_predict_base(prompt.len(), 0)
    }

    /// Computes a per-request `num_predict` cap for the `chat` path. `tool_overhead_tokens`
    /// is the real token count of the serialized tool-definition JSON actually being sent
    /// (computed at the call site, not guessed) — see `compute_num_predict_base`.
    fn compute_num_predict_chat(&self, messages: &[OllamaChatMessage], tool_overhead_tokens: u64) -> i32 {
        let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
        self.compute_num_predict_base(total_chars, tool_overhead_tokens)
    }

    /// Shared calculation: `max_predict_tokens - (char_count / PROMPT_CHARS_PER_TOKEN) -
    /// PROMPT_TOKEN_SAFETY_MARGIN - tool_overhead_tokens`, clamped to a positive floor.
    /// `saturating_sub` rather than a plain `-` — `char_count`'s real token cost can
    /// exceed this estimate (see `PROMPT_CHARS_PER_TOKEN`'s own doc comment), and on a
    /// prompt already this close to `max_predict_tokens`, an unsigned underflow here
    /// would silently wrap to a huge value in a release build instead of panicking,
    /// producing a nonsense `num_predict` that could reopen the exact unbounded-
    /// generation failure mode this whole mechanism exists to prevent.
    fn compute_num_predict_base(&self, char_count: usize, tool_overhead_tokens: u64) -> i32 {
        let estimated_tokens = (char_count as f64 / PROMPT_CHARS_PER_TOKEN) as u64;
        let reserved = estimated_tokens + PROMPT_TOKEN_SAFETY_MARGIN + tool_overhead_tokens;
        let remaining = (self.max_predict_tokens as u64).saturating_sub(reserved);
        remaining.max(MIN_NUM_PREDICT) as i32
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
    pub async fn generate(
        &self,
        prompt: String,
        think: Option<bool>,
        model: &str,
    ) -> Result<OllamaGenerateResponse, OllamaErrors> {
        let url = format!("{}/api/generate", self.base_url);
        let num_predict = self.compute_num_predict_generate(&prompt);
        let body = OllamaGenerateRequest {
            model: model.to_string(),
            prompt,
            stream: false,
            think: Value::Bool(think.unwrap_or(true)),
            options: OllamaOptions { num_predict },
        };

        tracing::info!("calling ollama /api/generate");
        let res = self.client.post(&url).json(&body).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/generate request failed");
            OllamaErrors::RequestFailed(e.to_string())
        })?;

        // reqwest's `send` only errors on transport-level failures (connection refused,
        // timeout, TLS) — an HTTP error status like 500 still comes back as `Ok`. Without
        // this check, a non-2xx response (whose body likely isn't our expected JSON shape)
        // would surface as a confusing decode error instead of the actual status.
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/generate returned a non-success status");
            return Err(OllamaErrors::UnexpectedStatus(status));
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
    /// before answering and defaults to enabled (no specific effort level — Ollama's
    /// own default) when not given — see `think_param` for exactly what gets sent on
    /// the wire for each `ThinkChoice` variant.
    pub async fn chat(
        &self,
        messages: Vec<OllamaChatMessage>,
        new_message: Option<OllamaChatMessage>,
        tools: &[&dyn Tool],
        think: Option<ThinkChoice>,
        model: &str,
    ) -> Result<OllamaChatResponse, OllamaErrors> {
        let url = format!("{}/api/chat", self.base_url);

        let definitions = Self::tool_definitions(tools);

        // Serialize the tool definitions to compact JSON (exactly what hits Ollama's wire)
        // and count the bytes → tokens for an accurate overhead estimate, computed before
        // `definitions` moves into `tools` below so nothing needs cloning for this.
        let tool_overhead_tokens = if definitions.is_empty() {
            0
        } else {
            let json_bytes = serde_json::to_string(&definitions).unwrap_or_default().len();
            (json_bytes as f64 / PROMPT_CHARS_PER_TOKEN).ceil() as u64
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

        let num_predict = self.compute_num_predict_chat(&messages, tool_overhead_tokens);
        let body = OllamaChatRequest {
            model: model.to_string(),
            messages,
            stream: false,
            think: think_param(think),
            tools,
            options: OllamaOptions { num_predict },
        };

        tracing::info!("calling ollama /api/chat");
        let res = self.client.post(&url).json(&body).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/chat request failed");
            OllamaErrors::RequestFailed(e.to_string())
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/chat returned a non-success status");
            return Err(OllamaErrors::UnexpectedStatus(status));
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
    /// why `mtp-proxy` implements its own `/api/show` (translating from `/props`) —
    /// this method works unchanged against either backend.
    pub async fn thinking_capability(&self, model: &str) -> Result<ThinkingCapability, OllamaErrors> {
        Ok(parse_thinking_capability(&self.chat_template(model).await?))
    }

    /// The tags the given `model`'s own chat template wraps a tool call in (e.g.
    /// `<tool_call>` and `</tool_call>`), discovered live the same way as
    /// `thinking_capability` — see `extract_tool_call_markers`. Empty if the template
    /// has none or can't be read, in which case nothing that looks for them can
    /// trigger: a failure to look is never treated as a finding.
    pub async fn tool_call_markers(&self, model: &str) -> Vec<String> {
        match self.chat_template(model).await {
            Ok(template) => extract_tool_call_markers(&template),
            Err(_) => vec![],
        }
    }

    /// The given `model`'s raw Jinja chat template, straight from Ollama's `/api/show`
    /// — see `thinking_capability` for why that's the real template and why it's read
    /// fresh every time.
    async fn chat_template(&self, model: &str) -> Result<String, OllamaErrors> {
        let url = format!("{}/api/show", self.base_url);
        let body = serde_json::json!({ "model": model });

        let res = self.client.post(&url).json(&body).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/show request failed");
            OllamaErrors::RequestFailed(e.to_string())
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/show returned a non-success status");
            return Err(OllamaErrors::UnexpectedStatus(status));
        }

        #[derive(Deserialize, Default)]
        struct ShowResponse {
            #[serde(default)]
            template: String,
        }
        let parsed: ShowResponse = decode_response(res).await?;
        Ok(parsed.template)
    }

    /// The models actually installed in this Ollama instance, from `/api/tags` — the
    /// authoritative "what can I switch to right now without pulling" list. Ollama's own
    /// fields (`details.parameter_size`/`quantization_level`) are passed straight through
    /// so the client can show/estimate requirements without a second round trip.
    pub async fn list_local_models(&self) -> Result<Vec<LocalModel>, OllamaErrors> {
        let url = format!("{}/api/tags", self.base_url);

        let res = self.client.get(&url).send().await.map_err(|e| {
            tracing::error!(error = %e, "ollama /api/tags request failed");
            OllamaErrors::RequestFailed(e.to_string())
        })?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            tracing::error!(%status, body, "ollama /api/tags returned a non-success status");
            return Err(OllamaErrors::UnexpectedStatus(status));
        }

        #[derive(Deserialize, Default)]
        struct TagsResponse {
            #[serde(default)]
            models: Vec<LocalModel>,
        }
        let parsed: TagsResponse = decode_response(res).await?;
        Ok(parsed.models)
    }

    /// Fetches ollama.com's public model library page. There is no API behind it, so
    /// `ModelLibrary` parses the returned HTML into a structured catalog.
    pub async fn fetch_catalog(&self) -> Result<String, OllamaErrors> {
        let res = self
            .client
            .get("https://ollama.com/library")
            .header("Accept", "text/html")
            .send()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "ollama.com/library request failed");
                OllamaErrors::RequestFailed(e.to_string())
            })?;

        if !res.status().is_success() {
            let status = res.status();
            tracing::error!(%status, "ollama.com/library returned a non-success status");
            return Err(OllamaErrors::UnexpectedStatus(status));
        }

        res.text().await.map_err(|e| OllamaErrors::DecodeFailed(e.to_string()))
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
            images: None,
        }
    }

    /// Builds a `user`-role message carrying attached images, base64-encoded (no
    /// data-URL prefix) — same as `user_message` otherwise. `images` empty is treated
    /// the same as no images at all (`None` on the wire), so callers don't need to
    /// branch on whether there's actually anything to attach.
    pub fn user_message_with_images(content: String, images: Vec<String>) -> OllamaChatMessage {
        OllamaChatMessage {
            images: (!images.is_empty()).then_some(images),
            ..Self::user_message(content)
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
            images: None,
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
            images: None,
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

/// The tags a chat template wraps a tool call in, as literal text: every `<name>` and
/// `</name>` where the template mentions a tag whose name contains `tool_call` or
/// `function_call` (Qwen's and Hermes-style templates spell theirs `<tool_call>` /
/// `</tool_call>`). Read from the template rather than assumed, so it follows whatever
/// model is active; a model whose template has no such tag yields nothing, and nothing
/// downstream fires for it. Order-preserving, no duplicates.
fn extract_tool_call_markers(template: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for (start, _) in template.match_indices('<') {
        let rest = &template[start + 1..];
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !rest[name.len()..].starts_with('>') {
            continue;
        }

        let lower = name.to_ascii_lowercase();
        if (lower.contains("tool_call") || lower.contains("function_call")) && !names.contains(&name) {
            names.push(name);
        }
    }

    names.into_iter().flat_map(|name| [format!("<{name}>"), format!("</{name}>")]).collect()
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

/// A caller-requested `think` setting, from the API boundary down to `chat`. Plain
/// `true`/`false` preserves this project's original behavior exactly (Ollama's own
/// default reasoning effort, no override) — every internal, non-user-facing call
/// site (compaction's fold check, `llm.read_image`, the messaging plugins) uses only
/// this form, deliberately: none of them have a UI behind them to pick a level from,
/// so there's nothing to decide and no reason to second-guess Ollama's default.
/// `Level` is a specific effort string (e.g. `"low"`), used only when a caller
/// actually has one to offer — today that's the frontend's thinking-mode selector,
/// via `Agent::chat`/`continue_chat`, populated from `OllamaService::thinking_capability`.
#[derive(Deserialize, Serialize, Clone, Debug, ToSchema)]
#[serde(untagged)]
pub enum ThinkChoice {
    Enabled(bool),
    Level(String),
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

/// What a model's own chat template actually supports for `think`, discovered by
/// inspecting its raw text rather than hardcoded per model family. `Graduated`
/// carries the exact accepted effort strings, in the order the template lists them
/// (e.g. `["xhigh", "medium", "low"]`) — a frontend selector should offer exactly
/// these, nothing assumed beyond them. `OnOff` means the template supports enabling/
/// disabling thinking but has no graduated-effort concept at all (no levels to
/// offer — a plain toggle is the most this model supports). `Unsupported` means no
/// thinking-control markers were found at all (not even on/off) — hide any thinking
/// control entirely for this model.
#[derive(Serialize, Clone, Debug, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThinkingCapability {
    Graduated { modes: Vec<String> },
    OnOff,
    Unsupported,
}

/// Parses a model's raw Jinja chat template text for what it actually supports —
/// this is real per-model detection, not a fact about one specific model baked into
/// code. Looks first for a graduated `reasoning_effort` enum (the pattern this
/// Unsloth/Qwen convention uses: `{%- if resolved_reasoning_effort not in ('xhigh',
/// 'medium', 'low') %}` right before a `raise_exception` naming the supported set —
/// `extract_reasoning_effort_levels` pulls the quoted literals straight out of that
/// `not in (...)` check, since that's the actual enforced logic, not just the
/// human-readable error text restating it). Falls back to a plain `enable_thinking`
/// substring check for "does this model support on/off at all" if no graduated
/// pattern is found — this is NOT a universal chat-template standard (other model
/// families phrase reasoning effort differently, or don't support it at all), so
/// failing to find the `reasoning_effort` pattern correctly degrades to `OnOff`
/// rather than claiming levels that don't exist; failing to find `enable_thinking`
/// either degrades further to `Unsupported` rather than showing a control that would
/// do nothing.
fn parse_thinking_capability(template: &str) -> ThinkingCapability {
    if let Some(modes) = extract_reasoning_effort_levels(template) {
        if !modes.is_empty() {
            return ThinkingCapability::Graduated { modes };
        }
    }
    if template.contains("enable_thinking") {
        return ThinkingCapability::OnOff;
    }
    ThinkingCapability::Unsupported
}

fn extract_reasoning_effort_levels(template: &str) -> Option<Vec<String>> {
    let anchor = template.find("reasoning_effort")?;
    let window = &template[anchor..];
    let not_in = window.find("not in (")?;
    let after = &window[not_in + "not in (".len()..];
    let close = after.find(')')?;
    let inside = &after[..close];

    let levels: Vec<String> = inside
        .split(',')
        .filter_map(|raw| {
            let trimmed = raw.trim().trim_matches(|c| c == '\'' || c == '"');
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .collect();

    (!levels.is_empty()).then_some(levels)
}

/// One installed model as `/api/tags` reports it, passed straight out over our own API
/// (hence `Serialize`/`ToSchema`, unlike the request/response mirror types) — the client
/// reads `details.parameter_size`/`quantization_level` to show and estimate a model's
/// requirements. Only the fields the UI actually uses are kept; Ollama sends more.
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct LocalModel {
    /// The pullable tag, e.g. `qwen2.5:0.5b` — what a chat's `model` gets set to.
    pub name: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub details: Option<LocalModelDetails>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct LocalModelDetails {
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub parameter_size: Option<String>,
    #[serde(default)]
    pub quantization_level: Option<String>,
}

#[derive(Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    think: Value,
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
    think: Value,
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
    /// `BTreeMap`, not `HashMap` — this gets rebuilt fresh on every `chat` call, and
    /// `HashMap`'s randomized per-instance hasher seed means its (and therefore
    /// `serde_json`'s) key order isn't stable across two builds of the same content.
    /// The chat template renders this into the literal prompt text, so an unstable
    /// order here changes the prompt's bytes on every single call — right alongside
    /// `Agent::advance`'s own prompt-cache fix, this is the other half of what made
    /// Ollama/llama.cpp's prefix cache fail to match almost immediately on every turn.
    properties: BTreeMap<String, OllamaToolProperty>,
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
    /// Base64-encoded image data (no data-URL prefix) — see `user_message_with_images`.
    /// Ollama never sends this back on a response message, so it's only ever `Some` on
    /// an outgoing `user`-role message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
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
    /// Ollama refused a model-management request and said why (a bad model name, a corrupt
    /// file, ...) — the text is meant to be shown to the person who asked.
    Rejected(StatusCode, String),
    /// A model-management operation failed for a reason of its own (an unreadable file, a pull
    /// that ended without success).
    Failed(String),
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
            OllamaErrors::Rejected(code, msg) => {
                ErrorService::new(if code.is_client_error() { StatusCode::BAD_REQUEST } else { StatusCode::BAD_GATEWAY }, msg)
            }
            OllamaErrors::Failed(msg) => ErrorService::new(StatusCode::BAD_GATEWAY, msg),
            OllamaErrors::DecodeFailed(msg) => {
                // The precise cause (including the raw body Ollama sent) is already
                // logged at the source in `decode_response` — this only has the
                // stringified message left, which isn't useful to log twice.
                ErrorService::internal(format!("failed to decode ollama response: {msg}"))
            }
        }
    }
}
