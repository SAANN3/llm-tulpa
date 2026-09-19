//! Command-line handling shared by every tool that runs a shell command
//! (`os.execute_command`, `os.start_job`): what a command needs approval for, what may
//! never run, how a command line is prepared, and how its output is bounded. The
//! platform-specific parts of actually running one live in `services::process`.

use std::collections::HashMap;

use serde_json::Value;

use crate::tools::base::{ResolvedScope, ScopeGrant, SharedBucket, ToolPermission};

/// `description()` is compile-time-fixed text, but whether this container is actually
/// scoped away from the host or not depends on how the backend was launched, not on
/// anything decided at compile time — the same binary can run either way. Detecting it
/// once at runtime (Docker always creates `/.dockerenv`) and picking the matching text
/// keeps the model's own understanding of what this tool can reach accurate either way,
/// instead of baking in whichever answer happened to be true when this was written.
pub(super) fn running_in_docker() -> bool {
    std::path::Path::new("/.dockerenv").exists()
}

/// Same cap and truncation-marker convention as `storage.read_file`'s
/// `MAX_READ_CHARS` — this tool had none at all until a real incident: `grep -r`
/// across a directory containing a few minified/single-line files (VS Code
/// extension bundles) matched a handful of megabytes-long lines; `| head -20`
/// still let all of them through (it caps *lines*, not bytes), producing an
/// 8MB+ tool result that got stored as a real chat message. On the next turn,
/// sending that message as history blew ~30x past the model's context window;
/// Ollama silently truncated it down to fit (no error anywhere in the pipeline
/// — this shipped with zero logging for that path), and what survived excluded
/// nearly the entire actual conversation, so the model's very next reply looked
/// like it had genuinely forgotten everything — confirmed via Postgres directly
/// (`prompt_eval_count` cratered from ~43,000 to ~6,800 between consecutive
/// calls in the same chat, no compaction summary ever involved). Capping stdout
/// and stderr independently here is the actual fix — matches `storage.read_file`
/// so a single command can never again silently blow the whole context budget.
pub(super) const MAX_OUTPUT_CHARS: usize = 40_000;

pub(super) fn truncate_output(content: String) -> (String, bool) {
    let total_chars = content.chars().count();
    if total_chars <= MAX_OUTPUT_CHARS {
        return (content, false);
    }
    let cropped: String = content.chars().take(MAX_OUTPUT_CHARS).collect();
    (
        format!(
            "{cropped}\n\n[... output truncated: showing the first {MAX_OUTPUT_CHARS} of {total_chars} characters ...]"
        ),
        true,
    )
}

/// The last `MAX_OUTPUT_CHARS` characters of `content`, for output where the newest
/// part is the part that matters (a job's log) — the counterpart of `truncate_output`,
/// which keeps the first.
pub(super) fn tail_output(content: String) -> (String, bool) {
    let total_chars = content.chars().count();
    if total_chars <= MAX_OUTPUT_CHARS {
        return (content, false);
    }
    let cropped: String = content.chars().skip(total_chars - MAX_OUTPUT_CHARS).collect();
    (
        format!(
            "[... earlier output truncated: showing the last {MAX_OUTPUT_CHARS} of {total_chars} characters ...]\n\n{cropped}"
        ),
        true,
    )
}

fn parse_command_for_scope(cmd: &str) -> Option<String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for part in parts.iter() {
        if part.starts_with('/') || part.starts_with('.') || part.starts_with('~') {
            return Some(part.to_string());
        }
    }
    None
}

/// The executable/command word a command starts with (e.g. `"python"` out of `"python
/// -c '1+1'"`) — what an approval actually covers. Deliberately just the first
/// whitespace-separated token, verbatim: doesn't strip a path prefix, doesn't look past
/// a `&&`/`;`/`|` at a second command chained after it, and two different ways of
/// spelling the same binary (`python` vs `/usr/bin/python3`) are two different grants.
/// Narrower than a human might expect in the chained-command case, which is the
/// direction to err in for a tool that runs arbitrary shell — the destructive-pattern
/// check above still applies regardless of what's approved here.
fn base_command(cmd: &str) -> Option<&str> {
    cmd.split_whitespace().next()
}

/// Each shell statement's own first word — a rough, not-a-real-parser stand-in for
/// "what command is actually about to run here," used only to keep the
/// destructive-pattern check below from matching a command name that merely appears
/// as ordinary text elsewhere in the line. Splitting on `;`/`&&`/`||`/`|`/newline
/// isn't a real shell grammar (doesn't handle quoting, subshells, ...) but is enough
/// to tell "`dd` is the command being run" apart from "`dd` is a word that happens to
/// sit between two other words" (e.g. a tool-name list like `sed dd od hexdump`,
/// which is exactly what triggered this: the old check was a raw substring search for
/// `"dd "`, which that list contains without a `dd` invocation anywhere in it).
fn statement_commands(cmd: &str) -> impl Iterator<Item = &str> {
    cmd.split(['\n', ';', '|'])
        .flat_map(|s| s.split("&&"))
        .filter_map(|stmt| stmt.split_whitespace().next())
}

/// Shell preamble (see `call_untyped`) that makes `apt-get`/`apt`/`dpkg` resolve to a
/// sudo'd invocation of themselves wherever they're actually called in the command
/// that follows — not just when one happens to be the very first word. Aliases (not
/// shell functions — this container's `/bin/sh` is dash, whose POSIX-strict function-
/// name grammar rejects the hyphen in `apt-get`, making `apt-get() { ...; }` a flat
/// syntax error there, breaking the *entire* script it's prepended to; `alias`, unlike
/// a function name, isn't restricted to identifier syntax) take part in normal command
/// lookup the same as a real binary would, so this transparently covers every position
/// a naive "is the command's first word one of these" check would miss: chained with
/// `&&`/`;`, inside a loop, after a pipe, however many times — confirmed live in dash
/// across all of those. (An earlier version spliced `sudo -n` into just the command's
/// leading word instead — that left a case like `apt-get update && apt-get install x`
/// with only the first `apt-get` elevated, so the second still hit dpkg's lock file
/// with a real "Permission denied.") The container's own non-root user is granted a
/// scoped, passwordless `sudo` covering exactly these three binaries (see the
/// Dockerfile's `pkg-mgmt` sudoers rule) — this exists so the model doesn't need to
/// already know this particular container requires `sudo` just to install a package.
/// Doesn't touch approval: `base_command`/`is_dangerous` still see `"apt-get"` etc. as
/// the command word being approved, same as any other — this only changes how it's
/// actually invoked once permitted. Defining these unconditionally rather than only
/// when the command appears to need them is harmless (an unused alias costs nothing)
/// and is exactly what avoids re-implementing shell parsing to detect every position
/// one might be called from.
const PACKAGE_MANAGER_SUDO_PREAMBLE: &str = "alias apt-get='sudo -n /usr/bin/apt-get'; \
     alias apt='sudo -n /usr/bin/apt'; \
     alias dpkg='sudo -n /usr/bin/dpkg';\n";

/// The command line to actually hand to the shell: `command` as the model wrote it,
/// with the package-manager preamble in front when running under Docker (see
/// `PACKAGE_MANAGER_SUDO_PREAMBLE`), and unchanged everywhere else.
pub(super) fn effective_command(command: &str) -> String {
    if running_in_docker() {
        format!("{PACKAGE_MANAGER_SUDO_PREAMBLE}{command}")
    } else {
        command.to_string()
    }
}

/// Whether `command` may run, given which command words have already been approved
/// (`approved_scope`: the `SharedBucket::ShellCommands` grant, if any). `auto_approved`
/// is a caller's own reason to skip the approval step — it does not skip the blocklist,
/// which applies regardless of what's been approved.
pub(super) fn check_command_permission(
    command: &str,
    workdir: Option<&str>,
    approved_scope: Option<&Value>,
    auto_approved: bool,
) -> ToolPermission {
    let cmd_lower = command.to_lowercase();
    let has_destructive_command = statement_commands(&cmd_lower)
        .any(|word| word == "dd" || word == "mkfs" || word.starts_with("mkfs."));
    if cmd_lower.contains("rm -rf /") || cmd_lower.contains("> /dev/sd") || has_destructive_command {
        return ToolPermission::Denied {
            reason: "Blocked obviously destructive command pattern".to_string(),
            escalation: None,
        };
    }

    if auto_approved {
        return ToolPermission::Allowed;
    }

    let Some(base) = base_command(command) else {
        return ToolPermission::Denied {
            reason: "couldn't find a command to run in an empty string".to_string(),
            escalation: None,
        };
    };

    let approved = approved_scope
        .and_then(|scope| scope.get(SharedBucket::ShellCommands.json_key()))
        .and_then(|commands| commands.as_object())
        .is_some_and(|commands| commands.contains_key(base));

    if approved {
        return ToolPermission::Allowed;
    }

    let path = workdir.map(str::to_string).or_else(|| parse_command_for_scope(command));
    let reason = match &path {
        Some(path) => format!("Execute command requires approval (path context: {path})"),
        None => "Execute command requires approval".to_string(),
    };

    ToolPermission::Denied {
        reason,
        escalation: Some(ScopeGrant {
            scope: ResolvedScope {
                own: None,
                shared: HashMap::from([(
                    SharedBucket::ShellCommands,
                    serde_json::json!({ SharedBucket::ShellCommands.json_key(): { base: true } }),
                )]),
            },
            ui_message: format!(
                "Allow running `{base}` (with any arguments) for the rest of this chat? \
                 Only this one command, exactly as typed — not general shell access."
            ),
        }),
    }
}
