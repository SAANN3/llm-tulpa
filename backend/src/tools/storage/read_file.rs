use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};

use super::{check_file_scope, normalize};

pub struct ReadFileTool;

/// Hard ceiling on how much of a file `read_file` hands back in one call. Measured in
/// characters rather than tokens — counting real tokens needs the model's own
/// tokenizer, and characters are a close enough proxy without that dependency. A single
/// huge file (a bundled/minified asset, a log, a lockfile) could otherwise single-
/// handedly blow the model's context budget; this is generous enough that ordinary
/// source files still come back whole.
const MAX_READ_CHARS: usize = 40_000;

#[derive(Deserialize, ToolParams)]
struct ReadFileArgs {
    #[tool(description = "Absolute or relative path to the file to read.")]
    path: String,
    #[tool(description = "How many CHARACTERS (not lines) to skip from the start of the file before reading — only for continuing past where an earlier call was cut off: pass the `next_offset` it returned. To read by line number use `start_line` instead. Defaults to 0 (start of the file).")]
    offset: Option<usize>,
    #[tool(description = "First line to read, counting from 1. With `end_line` it reads just those lines; without it, from there to the end (still cut at 40,000 characters, and then `next_start_line` says where to go on). Cannot be combined with `offset`.")]
    start_line: Option<usize>,
    #[tool(description = "Last line to read, inclusive, counting from 1. Defaults to the end of the file.")]
    end_line: Option<usize>,
    #[tool(description = "The most characters to return, at most 40,000 (also the default). Ask for a small number first to preview a file or a very long line (for example start_line=2, end_line=2, max_chars=300), and for more only if it is worth it. A line longer than this comes back cut, with a `next_offset` to read on.")]
    max_chars: Option<usize>,
}

#[derive(Serialize)]
struct ReadFileOut {
    content: String,
    /// `true` when there's more of the file after what `content` covers — the model
    /// needs to know its view is partial, not just get silently fed less than what's
    /// actually there.
    truncated: bool,
    /// The file's full length in characters, wherever this read started.
    total_chars: usize,
    /// Where to continue from (pass it back as `offset`) — only set when `truncated`.
    next_offset: Option<usize>,
    /// The file's length in lines, so a line range can be picked without a first read.
    total_lines: usize,
    /// A line-range read that stopped short of what was asked for (the size cap): the line to
    /// pass back as `start_line`.
    #[serde(skip_serializing_if = "Option::is_none")]
    next_start_line: Option<usize>,
}

/// How many characters a call may return: what it asked for through `max_chars`, never more than
/// `MAX_READ_CHARS` (which is also the default) and at least one.
fn effective_limit(requested: Option<usize>) -> usize {
    requested.unwrap_or(MAX_READ_CHARS).clamp(1, MAX_READ_CHARS)
}

/// What a `start_line`/`end_line` read covers.
#[derive(Debug, PartialEq)]
struct LineSlice {
    /// Lines `first..=last` (1-based, whole lines, newline kept) cut at `max_chars` on a line boundary
    text: String,
    /// The last line `text` reaches
    shown_to: usize,
    /// The next line to read, when the limit left some of the requested range out
    next_line: Option<usize>,
    /// The first line alone is longer than `max_chars`: `text` is the start of it, and the read goes on by
    /// character offset, since a line number can't point inside a line
    cut_in_line: bool,
}

fn line_slice(content: &str, first: usize, last: usize, max_chars: usize) -> LineSlice {
    let mut text = String::new();
    let mut count = 0;
    let mut line_no = first;
    for line in content.split_inclusive('\n').skip(first - 1).take(last.saturating_sub(first) + 1) {
        let len = line.chars().count();
        if count == 0 && len > max_chars {
            return LineSlice { text: line.chars().take(max_chars).collect(), shown_to: first, next_line: None, cut_in_line: true };
        }
        if count > 0 && count + len > max_chars {
            return LineSlice { text, shown_to: line_no - 1, next_line: Some(line_no), cut_in_line: false };
        }
        text.push_str(line);
        count += len;
        line_no += 1;
    }
    LineSlice { text, shown_to: line_no - 1, next_line: None, cut_in_line: false }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn function_name(&self) -> &str {
        "storage.read_file"
    }

    fn description(&self) -> &str {
        "Reads a file's contents as text. Fails if the file isn't valid UTF-8 text. \
         At most 40,000 characters come back (less with `max_chars`); a longer file or \
         line comes back truncated (see `truncated` in the response) — the file itself is \
         untouched, only what's returned here is cut short; pass the response's \
         `next_offset` as `offset` to read the next part."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReadFileArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: ReadFileArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, SharedBucket::StorageRead, scope.shared.get(&SharedBucket::StorageRead)))
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadFileArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;

        let total_chars = content.chars().count();
        let total_lines = content.split_inclusive('\n').count();
        let limit = effective_limit(args.max_chars);

        if args.start_line.is_some() || args.end_line.is_some() {
            if args.offset.is_some() {
                return Err(ToolError::FailedUnknown("use either `offset` (characters) or `start_line`/`end_line`, not both".into()));
            }
            let first = args.start_line.unwrap_or(1);
            let last = args.end_line.unwrap_or(total_lines.max(1));
            if first == 0 || last < first {
                return Err(ToolError::FailedUnknown("lines count from 1, and `end_line` can't be before `start_line`".into()));
            }
            if first > total_lines && total_lines > 0 {
                return Ok(serde_json::to_value(ReadFileOut {
                    content: format!("[line {first} is past the end of the file, which has {total_lines} lines]"),
                    truncated: false,
                    total_chars,
                    next_offset: None,
                    total_lines,
                    next_start_line: None,
                })?);
            }
            let slice = line_slice(&content, first, last, limit);
            let (content, next_offset) = if slice.cut_in_line {
                // Where the cut fell, counted in characters from the start of the file, so `offset` can go on from it
                let before: usize = content.split_inclusive('\n').take(first - 1).map(|line| line.chars().count()).sum();
                let end = before + slice.text.chars().count();
                (
                    format!(
                        "{}\n\n[... line {first} is longer than the {limit} characters returned here: showing its first {}; \
                         call again with offset={end} (characters from the start of the file) to read on ...]",
                        slice.text,
                        slice.text.chars().count()
                    ),
                    Some(end),
                )
            } else if let Some(next) = slice.next_line {
                (
                    format!(
                        "{}\n[... showing lines {first}-{} of {total_lines}; call again with start_line={next} to read on ...]",
                        slice.text, slice.shown_to
                    ),
                    None,
                )
            } else {
                (slice.text, None)
            };
            return Ok(serde_json::to_value(ReadFileOut {
                content,
                truncated: slice.cut_in_line || slice.next_line.is_some(),
                total_chars,
                next_offset,
                total_lines,
                next_start_line: slice.next_line,
            })?);
        }

        let offset = args.offset.unwrap_or(0);

        if offset >= total_chars && total_chars > 0 {
            return Ok(serde_json::to_value(ReadFileOut {
                content: format!("[offset {offset} is past the end of the file, which is {total_chars} characters long]"),
                truncated: false,
                total_chars,
                next_offset: None,
                total_lines,
                next_start_line: None,
            })?);
        }

        let window: String = content.chars().skip(offset).take(limit).collect();
        let end = offset + window.chars().count();
        let truncated = end < total_chars;

        let content = if truncated {
            format!(
                "{window}\n\n[... file truncated: showing characters {offset}-{end} of {total_chars}; \
                 call again with offset={end} to read on ...]"
            )
        } else {
            window
        };

        Ok(serde_json::to_value(ReadFileOut {
            content,
            truncated,
            total_chars,
            next_offset: truncated.then_some(end),
            total_lines,
            next_start_line: None,
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::{effective_limit, line_slice, LineSlice, MAX_READ_CHARS};

    const FILE: &str = "a\nbb\nccc\ndddd\n";

    fn slice(text: &str, shown_to: usize, next_line: Option<usize>, cut_in_line: bool) -> LineSlice {
        LineSlice { text: text.to_string(), shown_to, next_line, cut_in_line }
    }

    #[test]
    fn a_line_range_is_whole_lines_counted_from_one() {
        assert_eq!(line_slice(FILE, 2, 3, 100), slice("bb\nccc\n", 3, None, false));
        assert_eq!(line_slice(FILE, 1, 99, 100), slice(FILE, 4, None, false));
    }

    #[test]
    fn the_size_cap_stops_on_a_line_boundary_and_names_the_next_line() {
        // 3 + 4 chars fit in 8, the next 5 do not
        assert_eq!(line_slice(FILE, 2, 4, 8), slice("bb\nccc\n", 3, Some(4), false));
    }

    #[test]
    fn a_first_line_over_the_limit_comes_back_cut_and_never_whole() {
        assert_eq!(line_slice("xxxxxxxxxx\ny\n", 1, 2, 4), slice("xxxx", 1, None, true));
        // a later line that does not fit just waits for the next call
        assert_eq!(line_slice("y\nxxxxxxxxxx\n", 1, 2, 4), slice("y\n", 1, Some(2), false));
    }

    #[test]
    fn the_limit_defaults_to_the_maximum_and_never_goes_past_it() {
        assert_eq!(effective_limit(None), MAX_READ_CHARS);
        assert_eq!(effective_limit(Some(300)), 300);
        assert_eq!(effective_limit(Some(10 * MAX_READ_CHARS)), MAX_READ_CHARS);
        assert_eq!(effective_limit(Some(0)), 1);
    }
}
