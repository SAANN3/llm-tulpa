//! How long a model's reply may run and how long to wait for it, worked out from the context
//! window. Shared by every provider: whichever server answers, a model that never emits a stop
//! token must not be able to generate (or the request wait) forever.

use std::time::Duration;

use super::types::ChatMessage;

/// A request to a model server gets no response at all until generation finishes (`stream:
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
/// Fallback character-to-token ratio used when ground-truth token count is unavailable
/// (e.g. brand-new chats, pre-migration chats, or right after compaction resets the baseline).
/// Modern BPE tokenizers on English prose and code average ~3.0–3.5 chars/token.
const PROMPT_CHARS_PER_TOKEN: f64 = 3.2;

/// Lowest `num_predict` the per-request calculation will ever return. Sized to allow
/// reasoning models with extensive chains of thought (e.g. Qwen 3.8) to finish thinking
/// and produce an answer/tool call.
const MIN_NUM_PREDICT: u64 = 4096;

/// A flat token overhead added to the prompt-size estimate for content that character
/// count alone can't capture — tool definition JSON schemas, chat template special
/// tokens (`<s>`, `<|start_header|>`, etc.), and per-token overhead from the tokenizer
/// itself. Chosen to comfortably cover a typical tool-calling workload (6–8 tools with
/// full parameter schemas ≈ 3–5K tokens) with room for heavier setups; also doubles
/// for the `generate` path where it covers template overhead without tool definitions.
const PROMPT_TOKEN_SAFETY_MARGIN: u64 = 1024;

/// What `chars` characters of prompt come to in tokens, by the same ratio the reply cap is estimated with,
/// for a prompt nothing has measured yet.
pub fn estimated_prompt_tokens(chars: usize) -> u64 {
    (chars as f64 / PROMPT_CHARS_PER_TOKEN).ceil() as u64
}

fn request_timeout(num_predict: i32) -> Duration {
    Duration::from_secs_f64(num_predict as f64 / MIN_TOKENS_PER_SEC) + PROMPT_PROCESSING_BUFFER
}


/// The reply-length cap (`num_predict` / `max_tokens`) for a request, from the context window it
/// runs under: the window minus an estimate of what the prompt already takes. The window is the
/// provider's configured default unless the call names the model's own (`CallParams`).
#[derive(Clone, Copy)]
pub struct OutputBudget {
    default_context: u64,
}

impl OutputBudget {
    pub fn new(default_context: u64) -> Self {
        Self { default_context }
    }

    fn context(&self, requested: Option<u64>) -> u64 {
        requested.filter(|context| *context > 0).unwrap_or(self.default_context)
    }

    /// How long to wait for a reply capped at `num_predict` tokens.
    pub fn timeout_for(num_predict: i32) -> Duration {
        request_timeout(num_predict)
    }

    /// How long to wait when nothing narrower is known: a reply as long as the default window.
    pub fn default_timeout(&self) -> Duration {
        request_timeout(self.default_context as i32)
    }

    /// The token cost of tool definitions whose JSON is `json_bytes` long.
    pub fn tool_overhead_tokens(json_bytes: usize) -> u64 {
        (json_bytes as f64 / PROMPT_CHARS_PER_TOKEN).ceil() as u64
    }

    /// Computes a per-request `num_predict` cap for the `generate` path: `char_count`
    /// is just `prompt`'s own length, no tool overhead — see `base`.
    pub fn for_generate(&self, prompt: &str, context: Option<u64>) -> i32 {
        self.base(prompt.len(), 0, context)
    }

    /// Computes a per-request `num_predict` cap for the `chat` path.
    /// When `known_prompt_tokens` is provided (ground-truth from the previous request in the DB),
    /// only the newest message needs to be accounted for: the measurement is of the whole prompt,
    /// tool definitions included, so adding their cost again would take it away from the reply
    /// twice (about 9k tokens with this project's tool set, which on a 40k window is most of the
    /// room left at the compaction trigger). When `None`, the prompt is estimated from its characters
    /// and the tool definitions are added on top, since nothing measured them.
    pub fn for_chat(
        &self,
        messages: &[ChatMessage],
        tool_overhead_tokens: u64,
        known_prompt_tokens: Option<u64>,
        context: Option<u64>,
    ) -> i32 {
        let reserved = match known_prompt_tokens {
            Some(known) => {
                let newest_chars = messages.last().map_or(0, |m| m.content.len());
                let newest_tokens = (newest_chars as f64 / PROMPT_CHARS_PER_TOKEN).ceil() as u64;
                known + newest_tokens + PROMPT_TOKEN_SAFETY_MARGIN
            }
            None => {
                let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
                let estimated_tokens = (total_chars as f64 / PROMPT_CHARS_PER_TOKEN) as u64;
                estimated_tokens + PROMPT_TOKEN_SAFETY_MARGIN + tool_overhead_tokens
            }
        };
        let remaining = self.context(context).saturating_sub(reserved);
        remaining.max(MIN_NUM_PREDICT) as i32
    }

    /// Shared calculation: `context - (char_count / PROMPT_CHARS_PER_TOKEN) -
    /// PROMPT_TOKEN_SAFETY_MARGIN - tool_overhead_tokens`, clamped to a positive floor.
    /// `saturating_sub` rather than a plain `-` — `char_count`'s real token cost can
    /// exceed this estimate (see `PROMPT_CHARS_PER_TOKEN`'s own doc comment), and on a
    /// prompt already this close to the context window, an unsigned underflow here
    /// would silently wrap to a huge value in a release build instead of panicking,
    /// producing a nonsense `num_predict` that could reopen the exact unbounded-
    /// generation failure mode this whole mechanism exists to prevent.
    fn base(&self, char_count: usize, tool_overhead_tokens: u64, context: Option<u64>) -> i32 {
        let estimated_tokens = (char_count as f64 / PROMPT_CHARS_PER_TOKEN) as u64;
        let reserved = estimated_tokens + PROMPT_TOKEN_SAFETY_MARGIN + tool_overhead_tokens;
        let remaining = self.context(context).saturating_sub(reserved);
        remaining.max(MIN_NUM_PREDICT) as i32
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_measured_prompt_already_includes_the_tool_definitions() {
        let budget = OutputBudget::new(40_000);
        let messages = vec![ChatMessage::user("hi".to_string())];
        // 28,000 tokens measured, 9,000 of them tool definitions: 40,000 - 28,000 - 1,024 margin - 1 for the message
        let measured = budget.for_chat(&messages, 9_000, Some(28_000), None);
        assert_eq!(measured, 10_975);
        // Nothing measured: the estimate has no tool definitions in it, so they are added
        let estimated = budget.for_chat(&messages, 9_000, None, None);
        assert_eq!(estimated, 40_000 - 9_000 - 1_024);
    }

    #[test]
    fn test_chat_budget_known_tokens() {
        let budget = OutputBudget::new(98304);

        let messages = vec![ChatMessage::user("Hello there, please write a long function.".to_string())];

        // Case 1: Ground-truth prompt tokens known from prior turn = 67,139 tokens
        let num_predict = budget.for_chat(&messages, 500, Some(67139), None);
        // newest_chars = 42 chars -> ceil(42 / 3.2) = 14 tokens
        // reserved = 67139 + 14 + 1024 (PROMPT_TOKEN_SAFETY_MARGIN) = 68177: the measurement already has the tools in it
        // remaining = 98304 - 68177 = 30127
        assert_eq!(num_predict, 30127);

        // Case 2: Fallback when known_prompt_tokens is None
        // total_chars = 42 -> 42 / 3.2 = 13 tokens
        // reserved = 13 + 1024 + 500 = 1537
        // remaining = 98304 - 1537 = 96767
        let num_predict_fallback = budget.for_chat(&messages, 500, None, None);
        assert_eq!(num_predict_fallback, 96767);

        // Case 3: Respects MIN_NUM_PREDICT floor when prompt is near context window limit
        let num_predict_clamped = budget.for_chat(&messages, 500, Some(97000), None);
        assert_eq!(num_predict_clamped, MIN_NUM_PREDICT as i32);
    }

    #[test]
    fn test_a_models_own_context_replaces_the_default() {
        let budget = OutputBudget::new(98304);
        let messages = vec![ChatMessage::user("Hello there, please write a long function.".to_string())];

        // The same prompt in a model that runs a 32k window leaves 32768 - 1537 tokens
        assert_eq!(budget.for_chat(&messages, 500, None, Some(32768)), 31231);
        // A zero (an unset value that slipped through) falls back rather than leaving no room
        assert_eq!(budget.for_chat(&messages, 500, None, Some(0)), 96767);
    }
}
