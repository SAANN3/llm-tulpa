//! The Markdown form of a transcript.

use super::{AttachmentBody, ExportEntry, Speaker, Transcript};

pub(super) fn render(transcript: &Transcript) -> String {
    let mut out = format!("# {}\n\n", transcript.title.replace('\n', " "));
    out.push_str(&format!(
        "_Exported {} · {} message{}_\n\n",
        transcript.exported_at.format("%Y-%m-%d %H:%M %:z"),
        transcript.entries.len(),
        if transcript.entries.len() == 1 { "" } else { "s" },
    ));

    for entry in &transcript.entries {
        out.push_str(&entry_markdown(entry));
    }
    out
}

fn entry_markdown(entry: &ExportEntry) -> String {
    let who = match &entry.speaker {
        Speaker::User => "You".to_string(),
        Speaker::Assistant => "Assistant".to_string(),
        Speaker::Tool(name) => format!("Tool result · {name}"),
        Speaker::Notice => "Notice".to_string(),
    };
    let mut out = format!("---\n\n### {who} · {}\n\n", entry.time.format("%Y-%m-%d %H:%M"));

    if !entry.text.is_empty() {
        match entry.speaker {
            // A tool's output is data, not prose: kept verbatim, whatever it contains
            Speaker::Tool(_) => out.push_str(&fenced(&entry.text, "")),
            _ => {
                out.push_str(&entry.text);
                out.push_str("\n\n");
            }
        }
    }
    for call in &entry.calls {
        out.push_str(&format!("**Tool call** `{}`\n\n", call.name.replace('`', "'")));
        out.push_str(&fenced(&call.arguments, "json"));
    }
    for attachment in &entry.attachments {
        // A name goes into link text, where brackets would end it early
        let name = attachment.name.replace('[', "(").replace(']', ")");
        match &attachment.body {
            AttachmentBody::Image { markdown_source, .. } => out.push_str(&format!("![{name}]({markdown_source})\n\n")),
            AttachmentBody::Text(text) => {
                out.push_str(&format!("**File** `{}`\n\n", attachment.name.replace('`', "'")));
                out.push_str(&fenced(text, ""));
            }
            AttachmentBody::Listed { path } => out.push_str(&format!("[{name}]({path})\n\n")),
        }
    }
    if entry.omitted_attachments > 0 {
        out.push_str(&format!(
            "_{} attachment{} not included_\n\n",
            entry.omitted_attachments,
            if entry.omitted_attachments == 1 { "" } else { "s" },
        ));
    }
    out
}

/// A code block whose fence is longer than any run of backticks inside it, so the content can't
/// end the block early
fn fenced(text: &str, language: &str) -> String {
    let longest_run = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat((longest_run + 1).max(3));
    format!("{fence}{language}\n{text}\n{fence}\n\n")
}
