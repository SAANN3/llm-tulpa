//! A diagnostic for the prompt cache: llama.cpp (which serves Ollama's models and llama-server
//! alike) reuses its KV cache only up to the first token that differs from the previous request.

use std::collections::HashMap;
use std::sync::Mutex;

use super::tool_defs::ToolDefinition;
use super::types::ChatMessage;

/// Remembers the previous chat request per model, as one JSON string for the tool definitions
/// followed by one per message — what `check` diffs the next request against.
#[derive(Default)]
pub struct PrefixWatch {
    last_request: Mutex<HashMap<String, Vec<String>>>,
}

impl PrefixWatch {
    /// Logs when a chat request stops being an append-only extension of the previous one
    /// to the same model: llama.cpp reuses its KV cache only up to the first differing
    /// token, so any earlier rewrite (a compaction fold, a replayed message rendering
    /// differently, a reordered tool list) re-pays the whole prompt after that point.
    /// Entry 0 is the tool definitions, entry `n` is message `n - 1`. Comparing per-message
    /// JSON is a proxy for the rendered prompt (the chat template can additionally drop
    /// reasoning of messages before the last user message), so a divergence reported here
    /// is certain, while a cache miss with no report points at the template.
    pub fn check(&self, model: &str, tools: &Option<Vec<ToolDefinition>>, messages: &[ChatMessage]) {
        let mut current = vec![serde_json::to_string(tools).unwrap_or_default()];
        current.extend(messages.iter().map(|m| serde_json::to_string(m).unwrap_or_default()));

        // Best-effort diagnostics: a poisoned lock just skips the report.
        let Ok(mut last) = self.last_request.lock() else { return };
        if let Some(previous) = last.get(model) {
            let common = previous.iter().zip(&current).take_while(|(a, b)| a == b).count();
            if common < previous.len() {
                let (old, new) = (&previous[common], current.get(common));
                let offset = new.map_or(0, |new| old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count());
                let excerpt = |text: &str| {
                    let mut start = offset.saturating_sub(80);
                    while !text.is_char_boundary(start) {
                        start -= 1;
                    }
                    text.chars().skip(text[..start].chars().count()).take(200).collect::<String>()
                };
                tracing::info!(
                    model,
                    first_diff_entry = common,
                    diff_byte_offset = offset,
                    previous_entries = previous.len(),
                    current_entries = current.len(),
                    prefix_bytes = previous[..common].iter().map(String::len).sum::<usize>(),
                    old_excerpt = excerpt(old),
                    new_excerpt = new.map(|n| excerpt(n)).unwrap_or_else(|| "<request ended here>".into()),
                    "chat request rewrites the previous request's prefix (entry 0 = tools, n = message n-1); llama.cpp will re-evaluate from here"
                );
            }
        }
        last.insert(model.to_string(), current);
    }
}
