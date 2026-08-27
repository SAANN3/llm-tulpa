use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Characters MarkdownV2 requires escaping wherever they appear as literal text (i.e.
/// outside the syntax markers this module emits itself) — per Telegram's own list:
/// https://core.telegram.org/bots/api#markdownv2-style
const SPECIAL: &[char] = &['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!', '\\'];

fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Inside a code span/block, MarkdownV2 only requires escaping backtick and backslash
/// — everything else (including the characters in `SPECIAL`) is literal as-is.
fn escape_code(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '`' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Inside a link's `(...)` destination, MarkdownV2 only requires escaping a literal
/// `)` or `\` — anything else in a URL (`.`, `-`, `_`, ...) is not special there.
fn escape_url(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == ')' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Converts CommonMark-ish text (what the model actually produces) into Telegram's
/// MarkdownV2 dialect: real formatting (bold/italic/strikethrough/code/links) is
/// re-emitted as MarkdownV2's own syntax, while every other character coming from the
/// source text is escaped so stray punctuation the model didn't intend as markup can't
/// make Telegram reject the whole message (MarkdownV2 requires escaping several plain
/// ASCII punctuation characters anywhere they appear outside of intentional markup, or
/// the send fails outright — see `SPECIAL` above).
///
/// Headings and lists have no MarkdownV2 equivalent, so they're flattened to bold text
/// and bulleted/numbered plain lines respectively — not pixel-perfect, but always valid
/// (never rejected) output, which matters more for a chat reply than exact fidelity.
pub fn to_telegram_markdown_v2(source: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let mut out = String::with_capacity(source.len());
    // `Some(n)` = next ordinal for an ordered list at this depth, `None` = unordered —
    // pushed/popped as `List`/`Item` tags nest, so a numbered list inside a bulleted
    // one (or vice versa) still gets the right marker at each level.
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    // A link's destination arrives on `Start(Tag::Link { dest_url, .. })`, but MarkdownV2
    // needs it written *after* the link text, at `TagEnd::Link` — held here in between.
    // A plain `Option`, not a stack: CommonMark links can't nest inside other links.
    let mut pending_link_url: Option<String> = None;
    // A fenced code block's contents arrive as plain `Event::Text` (unlike an inline
    // code span, which gets its own `Event::Code`) — this is what tells the `Text` arm
    // below to escape it as code (only backtick/backslash) instead of as ordinary text
    // (the full `SPECIAL` set), so e.g. `fn main() {}` inside a block doesn't come out
    // as `fn main\(\) \{\}`.
    let mut in_code_block = false;

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { .. } => out.push('*'),
                Tag::CodeBlock(kind) => {
                    out.push_str("```");
                    if let CodeBlockKind::Fenced(lang) = kind {
                        out.push_str(&lang);
                    }
                    out.push('\n');
                    in_code_block = true;
                }
                Tag::List(start) => list_stack.push(start),
                Tag::Item => {
                    match list_stack.last_mut() {
                        Some(Some(n)) => {
                            out.push_str(&format!("{n}\\. "));
                            *n += 1;
                        }
                        _ => out.push_str("• "),
                    }
                }
                Tag::Emphasis => out.push('_'),
                Tag::Strong => out.push('*'),
                Tag::Strikethrough => out.push('~'),
                Tag::Link { dest_url, .. } => {
                    out.push('[');
                    pending_link_url = Some(dest_url.to_string());
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => out.push_str("\n\n"),
                TagEnd::Heading(_) => out.push_str("*\n\n"),
                TagEnd::CodeBlock => {
                    in_code_block = false;
                    out.push_str("```\n\n");
                }
                TagEnd::List(_) => {
                    list_stack.pop();
                    out.push('\n');
                }
                TagEnd::Item => out.push('\n'),
                TagEnd::Emphasis => out.push('_'),
                TagEnd::Strong => out.push('*'),
                TagEnd::Strikethrough => out.push('~'),
                TagEnd::Link => {
                    if let Some(url) = pending_link_url.take() {
                        out.push_str("](");
                        out.push_str(&escape_url(&url));
                        out.push(')');
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    out.push_str(&escape_code(&text));
                } else {
                    out.push_str(&escape_text(&text));
                }
            }
            Event::Code(code) => {
                out.push('`');
                out.push_str(&escape_code(&code));
                out.push('`');
            }
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::Rule => out.push_str("\\-\\-\\-\n\n"),
            _ => {}
        }
    }

    out.trim().to_string()
}
