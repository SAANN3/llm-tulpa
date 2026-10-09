//! The shapes every model provider speaks in: the chat messages and replies the rest of the app
//! sends and reads, the capability and model-listing types, and the errors. Nothing here knows a
//! provider's wire format — each provider converts to and from these.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::services::error::ErrorService;

// Shared between outgoing request messages and the reply's `message` field — covers what a
// message looks like on either side of the wire.
#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ModelToolCall>>,
    /// Only present on `tool`-role messages — which tool produced `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Populated by `chat` after decoding, from `split_thinking` — always `None` on
    /// outgoing messages we construct ourselves (`ChatMessage::user`/`ChatMessage::tool`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// Base64-encoded image data (no data-URL prefix) — see `ChatMessage::user_with_images`.
    /// Ollama never sends this back on a response message, so it's only ever `Some` on
    /// an outgoing `user`-role message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModelToolCall {
    pub id: String,
    pub function: ModelToolCallFunction,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModelToolCallFunction {
    #[serde(default)]
    pub index: Option<u32>,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl ChatMessage {
    /// Builds a `user`-role message from plain text, so callers can hand `chat` a
    /// history/new-message pair without constructing `ChatMessage` by hand.
    pub fn user(content: String) -> ChatMessage {
        ChatMessage {
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
    pub fn user_with_images(content: String, images: Vec<String>) -> ChatMessage {
        ChatMessage {
            images: (!images.is_empty()).then_some(images),
            ..Self::user(content)
        }
    }

    /// Builds a `system`-role message from plain text, so callers can hand `chat` a
    /// history/new-message pair without constructing `ChatMessage` by hand.
    pub fn system(content: String) -> ChatMessage {
        ChatMessage {
            role: "system".to_string(),
            content,
            tool_calls: None,
            tool_name: None,
            thinking: None,
            images: None,
        }
    }

    /// Builds a `tool`-role message carrying a tool's result, so callers can hand `chat`
    /// a history/new-message pair without constructing `ChatMessage` by hand.
    /// `content` is a JSON value (a tool's result) rather than a `String` since that's
    /// what `Tool::call_untyped` returns; the wire formats want it stringified.
    pub fn tool(tool_name: String, content: Value) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_name: Some(tool_name),
            thinking: None,
            images: None,
        }
    }
}

/// A caller-requested `think` setting, from the API boundary down to `chat`. Plain
/// `true`/`false` preserves this project's original behavior exactly (Ollama's own
/// default reasoning effort, no override) — every internal, non-user-facing call
/// site (compaction's fold check, `llm.read_image`, the messaging plugins) uses only
/// this form, deliberately: none of them have a UI behind them to pick a level from,
/// so there's nothing to decide and no reason to second-guess Ollama's default.
/// `Level` is a specific effort string (e.g. `"low"`), used only when a caller
/// actually has one to offer — today that's the frontend's thinking-mode selector,
/// via `Agent::chat`/`continue_chat`, populated from `LlmProvider::thinking_capability`.
#[derive(Deserialize, Serialize, Clone, Debug, ToSchema)]
#[serde(untagged)]
pub enum ThinkChoice {
    Enabled(bool),
    Level(String),
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

/// A model the backend has loaded right now, from `/api/ps`. Only what the stats page shows is
/// kept. The llama.cpp provider answers this for llama-server (which always has exactly its one model loaded
/// and doesn't report memory use), so every field but the name may be absent.
#[derive(Deserialize, Default)]
pub struct RunningModel {
    pub name: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub size_vram: Option<u64>,
    /// When the backend will unload the model if it stays idle, RFC 3339.
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
}

/// What llama-server has done since it started, which only llama.cpp reports (Ollama has no equivalent).
#[derive(Deserialize, Default)]
pub struct LlamaServerStats {
    #[serde(default)]
    pub model_file: Option<String>,
    #[serde(default)]
    pub slots_total: Option<u64>,
    #[serde(default)]
    pub slots_processing: Option<u64>,
    #[serde(default)]
    pub prompt_tokens_total: Option<f64>,
    #[serde(default)]
    pub prompt_tokens_cached_total: Option<f64>,
    #[serde(default)]
    pub prompt_tokens_per_second: Option<f64>,
    #[serde(default)]
    pub predicted_tokens_total: Option<f64>,
    #[serde(default)]
    pub predicted_tokens_per_second: Option<f64>,
    #[serde(default)]
    pub draft_tokens_total: Option<f64>,
    #[serde(default)]
    pub draft_tokens_accepted_total: Option<f64>,
}

#[derive(Deserialize, Default)]
pub struct RunningModels {
    #[serde(default)]
    pub models: Vec<RunningModel>,
    #[serde(default)]
    pub llama_server: Option<LlamaServerStats>,
}

/// Sampling settings for one call. A field left `None` is not sent, so the server's own default (or
/// the model's) stays in force.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sampling {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub min_p: Option<f32>,
    pub repeat_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub seed: Option<i64>,
}

/// A model server configuration a caller needs running: the launch profile, the model file it
/// applies to, and who is asking (named in the message another user gets when this one has the
/// server busy).
#[derive(Clone, Debug)]
pub struct LaunchRequest {
    pub profile: crate::services::launch_store::LaunchProfile,
    pub model_file: std::path::PathBuf,
    /// The model folder, which a profile's projector file is relative to
    pub model_root: std::path::PathBuf,
    pub holder: String,
}

/// What a call may override about how the model runs it. `Default` overrides nothing.
#[derive(Clone, Debug, Default)]
pub struct CallParams {
    /// The launch the call needs the model server to be running under. `None` runs it on whatever
    /// is loaded (see `LlamaRuntime::acquire`): the app's one-shot prompts don't care which.
    pub launch: Option<LaunchRequest>,
    /// The context window the model runs under, when it isn't the provider's configured default —
    /// what the reply-length cap is worked out from.
    pub context_length: Option<u64>,
    pub sampling: Sampling,
}

/// One reply from a model, whichever provider produced it. Built by the provider from its own
/// wire format (see `ChatResponse::new`).
pub struct ChatResponse {
    pub model: String,
    pub created_at: String,
    pub message: ChatMessage,
    pub done_reason: Option<String>,
    metrics: ResponseMetrics,
}

impl ChatResponse {
    pub fn new(
        model: String,
        created_at: String,
        message: ChatMessage,
        done_reason: Option<String>,
        metrics: ResponseMetrics,
    ) -> Self {
        Self { model, created_at, message, done_reason, metrics }
    }

    /// How many tokens this call's prompt used, if the provider reported it — the real
    /// number (not an estimate), used to decide whether a chat's history is getting
    /// close enough to the context ceiling to be worth compacting. `None` on the rare
    /// response that omits metrics entirely, same as the providers' metric logging
    /// already tolerates.
    pub fn prompt_eval_count(&self) -> Option<u64> {
        self.metrics.prompt_eval_count
    }

    /// How many tokens the model generated in the response, if reported.
    pub fn eval_count(&self) -> Option<u64> {
        self.metrics.eval_count
    }

    /// How long generating the reply took, in milliseconds, if reported.
    pub fn eval_duration_ms(&self) -> Option<i64> {
        nanos_to_ms(self.metrics.eval_duration)
    }

    /// How long evaluating the prompt took, in milliseconds, if reported. Includes only the part
    /// the server had to compute: a prompt it already held in its cache costs next to nothing.
    pub fn prompt_eval_duration_ms(&self) -> Option<i64> {
        nanos_to_ms(self.metrics.prompt_eval_duration)
    }

    /// How many prompt tokens were actually evaluated, if the provider says — Ollama's own
    /// `prompt_eval_count` can't be told apart from a cached prompt's size.
    pub fn prompt_processed_tokens(&self) -> Option<i64> {
        self.metrics.prompt_eval_processed.map(|n| i64::try_from(n).unwrap_or(i64::MAX))
    }

    /// How long loading the model took, in milliseconds, if reported — zero when it was resident.
    pub fn load_duration_ms(&self) -> Option<i64> {
        nanos_to_ms(self.metrics.load_duration)
    }

    /// The call waited `load_ms` for its model to load before it was sent: a provider that loads the
    /// model itself (Ollama) reports this in the response; one whose server the backend starts
    /// (llama.cpp) can't, so the backend's own measurement is put in its place.
    pub fn with_load_ms(mut self, load_ms: Option<i64>) -> Self {
        if let Some(ms) = load_ms {
            self.metrics.load_duration = Some(u64::try_from(ms).unwrap_or(0).saturating_mul(1_000_000));
        }
        self
    }
}

fn nanos_to_ms(nanos: Option<u64>) -> Option<i64> {
    nanos.map(|ns| i64::try_from(ns / 1_000_000).unwrap_or(i64::MAX))
}

/// A piece of a reply as the model writes it, for showing it while it is written (see `LlmProvider::chat`'s
/// `on_piece`). Pieces are only shown: the reply is used once it is complete, the same as without them.
#[derive(Clone, Debug)]
pub enum ReplyPiece {
    Thinking(String),
    Text(String),
}

/// Told about each piece of a reply as it arrives
pub type OnPiece<'a> = &'a (dyn Fn(ReplyPiece) + Send + Sync);

/// What a provider reports about one call: token counts, and durations in nanoseconds. Every field
/// is optional because providers differ in what they say.
#[derive(Default, Clone, Debug)]
pub struct ResponseMetrics {
    pub load_duration: Option<u64>,
    pub prompt_eval_count: Option<u64>,
    pub prompt_eval_duration: Option<u64>,
    /// How many prompt tokens the server actually evaluated, when it can tell that from the whole
    /// prompt's size (a prompt served from its cache costs next to nothing).
    pub prompt_eval_processed: Option<u64>,
    pub eval_count: Option<u64>,
    pub eval_duration: Option<u64>,
}

/// A one-shot completion (`LlmProvider::generate`), whichever provider produced it.
pub struct GenerateResponse {
    pub model: String,
    pub created_at: String,
    pub response: String,
    /// The model's reasoning trace, when it produced one and the provider could separate it.
    pub thinking: Option<String>,
}

/// Failure modes for a call to a model provider, kept distinct so `From<LlmErrors> for
/// ErrorService` below can map each to an appropriate HTTP status rather than collapsing
/// everything to a generic 500. The first field of the transport-level variants is the provider's
/// name, so a message says whose server failed.
pub enum LlmErrors {
    RequestFailed(&'static str, String),
    /// A non-2xx response the provider didn't explain via its own error JSON (that's
    /// `Rejected`) — the raw response body, whatever it was, so a caller like
    /// `llm.read_image`'s tool error can actually say why instead of just the
    /// status code (e.g. llama.cpp/Ollama's own "image input is not supported -
    /// hint: if this is unexpected, you may need to provide the mmproj").
    UnexpectedStatus(&'static str, StatusCode, String),
    DecodeFailed(&'static str, String),
    /// A provider refused a model-management request and said why (a bad model name, a corrupt
    /// file, ...) — the text is meant to be shown to the person who asked.
    Rejected(StatusCode, String),
    /// A model-management operation failed for a reason of its own (an unreadable file, a pull
    /// that ended without success).
    Failed(String),
    /// The provider's server couldn't be made ready for the call (not installed, busy with another
    /// user, failed to load): carried through as it is, status and message included.
    Unavailable(ErrorService),
}

impl From<LlmErrors> for ErrorService {
    fn from(err: LlmErrors) -> Self {
        match err {
            LlmErrors::Unavailable(err) => err,
            LlmErrors::RequestFailed(provider, msg) => {
                // Transport-level failure (connection refused, timeout, ...) — not
                // something a caller did wrong, so the precise cause only matters here,
                // in the logs, not in the 500 body they get back.
                tracing::error!("failed to reach {provider}: {msg}");
                ErrorService::internal(format!("failed to reach {provider}: {msg}"))
            }
            LlmErrors::UnexpectedStatus(provider, code, body) => ErrorService::new(
                StatusCode::BAD_GATEWAY,
                format!("{provider} returned status {code}: {body}"),
            ),
            LlmErrors::Rejected(code, msg) => {
                ErrorService::new(if code.is_client_error() { StatusCode::BAD_REQUEST } else { StatusCode::BAD_GATEWAY }, msg)
            }
            LlmErrors::Failed(msg) => ErrorService::new(StatusCode::BAD_GATEWAY, msg),
            LlmErrors::DecodeFailed(provider, msg) => {
                // The precise cause (including the raw body the provider sent) is already
                // logged at the source — this only has the stringified message left, which
                // isn't useful to log twice.
                ErrorService::internal(format!("failed to decode {provider} response: {msg}"))
            }
        }
    }
}
