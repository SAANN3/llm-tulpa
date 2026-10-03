//! Reading a model's chat template and its raw output, which is the same job whichever provider
//! served it: what thinking control the template supports, which tags it wraps a tool call in, and
//! pulling a `</think>`-delimited reasoning block back out of text.

use super::types::ThinkingCapability;

/// The tags a chat template wraps a tool call in, as literal text: every `<name>` and
/// `</name>` where the template mentions a tag whose name contains `tool_call` or
/// `function_call` (Qwen's and Hermes-style templates spell theirs `<tool_call>` /
/// `</tool_call>`). Read from the template rather than assumed, so it follows whatever
/// model is active; a model whose template has no such tag yields nothing, and nothing
/// downstream fires for it. Order-preserving, no duplicates.
pub(super) fn extract_tool_call_markers(template: &str) -> Vec<String> {
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
pub(super) fn ensure_thinking_split(existing_thinking: Option<String>, content: String) -> (Option<String>, String) {
    if !content.contains("</think>") {
        return (existing_thinking, content);
    }

    let (derived_thinking, clean_content) = split_thinking(&content);
    (existing_thinking.or(derived_thinking), clean_content)
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
pub(super) fn parse_thinking_capability(template: &str) -> ThinkingCapability {
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

#[cfg(test)]
mod tests {
    use super::*;

    const QWEN: &str = "{%- if enable_thinking %}<think>{% endif %} <tool_call> </tool_call> {%- if resolved_reasoning_effort not in ('xhigh', 'medium', 'low') %}{{ raise_exception('bad') }}{% endif %}";

    #[test]
    fn a_graduated_template_lists_its_effort_levels() {
        match parse_thinking_capability(QWEN) {
            ThinkingCapability::Graduated { modes } => assert_eq!(modes, vec!["xhigh", "medium", "low"]),
            _ => panic!("expected graduated thinking"),
        }
    }

    #[test]
    fn thinking_without_levels_is_on_or_off_and_none_is_unsupported() {
        assert!(matches!(parse_thinking_capability("{% if enable_thinking %}<think>{% endif %}"), ThinkingCapability::OnOff));
        assert!(matches!(parse_thinking_capability("{{ messages }}"), ThinkingCapability::Unsupported));
    }

    #[test]
    fn tool_call_markers_come_from_the_template() {
        assert_eq!(extract_tool_call_markers(QWEN), vec!["<tool_call>", "</tool_call>"]);
        assert_eq!(extract_tool_call_markers("<function_call> and </function_call>"), vec!["<function_call>", "</function_call>"]);
        assert!(extract_tool_call_markers("<b>bold</b>").is_empty());
    }

    #[test]
    fn a_reasoning_block_is_split_out_of_the_text() {
        let (thinking, content) = split_thinking("first I think</think>\n\nthe answer");
        assert_eq!((thinking.as_deref(), content.as_str()), (Some("first I think"), "the answer"));
        // Cut off mid-thought: nothing safe to split
        assert_eq!(split_thinking("still thinking"), (None, "still thinking".to_string()));
    }

    #[test]
    fn the_providers_own_thinking_wins_over_the_derived_one() {
        let (thinking, content) = ensure_thinking_split(Some("provider's".into()), "x</think>y".into());
        assert_eq!((thinking.as_deref(), content.as_str()), (Some("provider's"), "y"));
        let (thinking, content) = ensure_thinking_split(None, "x</think>y".into());
        assert_eq!((thinking.as_deref(), content.as_str()), (Some("x"), "y"));
        assert_eq!(ensure_thinking_split(None, "plain".into()), (None, "plain".to_string()));
    }
}
