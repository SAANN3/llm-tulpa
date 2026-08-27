//! Block-splitting shared by every provider that has to break a long reply into
//! several platform messages (Telegram's `TELEGRAM_MAX_MESSAGE_CHARS`, Discord's
//! `DISCORD_MAX_MESSAGE_CHARS`, …). Fence-tracking here is provider-agnostic — it only
//! cares about `\n\n` and literal ` ``` ` runs, never about a provider's own escaping
//! rules — so it lives here once rather than once per provider. What differs per
//! provider (how an oversized single block gets hard-cut, and whether that needs to be
//! escape-pair-aware) stays local to that provider's own module.

/// Splits `text` on `"\n\n"` into top-level blocks (paragraphs, code blocks, lists, …),
/// except a blank line *inside* a fenced code block never counts as a boundary — code
/// content commonly has its own blank lines (e.g. between functions), and treating
/// those as top-level block breaks would put a code block's closing ` ``` ` in a
/// different block than its opening one, which can then end up in a different chunk
/// than the opener. Tracked by toggling on every literal ` ``` ` — a code block's own
/// open/close is the only place either provider's markdown conversion ever emits that
/// exact 3-backtick run (an inline code span uses one backtick, and any backtick inside
/// code *content* is individually escaped, never left as a bare run of three), so it's
/// an unambiguous marker for "entering/leaving a code block" here.
pub fn split_top_level_blocks(text: &str) -> Vec<&str> {
    let fence_positions: Vec<usize> = text.match_indices("```").map(|(pos, _)| pos).collect();

    let mut blocks = Vec::new();
    let mut block_start = 0;
    let mut search_from = 0;
    while let Some(rel_idx) = text[search_from..].find("\n\n") {
        let idx = search_from + rel_idx;
        let fences_before = fence_positions.iter().take_while(|&&pos| pos < idx).count();
        if fences_before % 2 == 0 {
            blocks.push(&text[block_start..idx]);
            block_start = idx + 2;
        }
        search_from = idx + 2;
    }
    blocks.push(&text[block_start..]);

    blocks
}

/// If `block` is a single, self-contained fenced code block — exactly what
/// `split_top_level_blocks` keeps intact as one block when a provider's own markdown
/// conversion emitted one — returns its language tag (possibly empty) and content
/// separately. `None` for anything else (plain text, a list, …).
pub fn strip_code_fence(block: &str) -> Option<(&str, &str)> {
    let after_open = block.strip_prefix("```")?;
    let newline_idx = after_open.find('\n')?;
    let lang = &after_open[..newline_idx];
    let content = after_open[newline_idx + 1..].strip_suffix("```")?;
    Some((lang, content))
}
