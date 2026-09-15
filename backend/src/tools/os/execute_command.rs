use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, ScopeGrant, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};
use crate::tools::storage::normalize;

/// `description()` is compile-time-fixed text, but whether this container is actually
/// scoped away from the host or not depends on how the backend was launched, not on
/// anything decided at compile time — the same binary can run either way. Detecting it
/// once at runtime (Docker always creates `/.dockerenv`) and picking the matching text
/// keeps the model's own understanding of what this tool can reach accurate either way,
/// instead of baking in whichever answer happened to be true when this was written.
fn running_in_docker() -> bool {
    std::path::Path::new("/.dockerenv").exists()
}

fn description_text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        if running_in_docker() {
            "Executes a shell command and returns stdout, stderr, and exit code — each capped \
             independently at 40,000 characters (see `stdout_truncated`/`stderr_truncated`); a \
             command whose real output is bigger than that gets cut off, not silently sent in \
             full. Be careful with anything that can match inside large or single-line files \
             (e.g. `grep -r` across a directory with minified/bundled files) — `| head -N` \
             only limits line *count*, not size, so a handful of very long matched lines can \
             still produce a huge result; prefer a narrower search path or piping through \
             something byte-bounded (e.g. `| head -c 5000`) when the target directory's \
             contents aren't known ahead of time. Runs inside this backend's own container, \
             not directly on the host machine, without touching the real host's package state, \
             and can read/write any path the storage.* tools can reach (the container shares \
             that same filesystem access). It cannot do host-system-wide things: no host \
             package manager, no host systemd/service control, no host-level `docker` \
             commands.\n\n\
             Already installed, permanently (baked into the image, survives every restart): \
             python3 (+pip, +venv), nodejs (+npm), go, rustc (+cargo), gcc/make \
             (build-essential), git, jq, unzip/zip, curl, wget, poppler-utils (pdftotext), \
             ripgrep (rg), fd, tree, sqlite3, docx2txt, gnumeric (ssconvert — .xlsx/.xls to \
             .csv), and a real headless Chromium via Playwright (browser binary already \
             downloaded — a fresh `npm install playwright` in a scratch directory is fast and \
             won't re-download it; needed locally rather than global because Node's `import` \
             resolution doesn't see a global npm install the way `require` can) for navigating \
             a real page and screenshotting it, then reading that screenshot with \
             llm.read_image. Don't apt-get/pip/npm install any of these, they're already \
             there.\n\n\
             `DISPLAY` is also set, pointing at the user's actual real screen — a GUI app \
             launched from here (e.g. Playwright's chromium.launch with `headless: false` \
             instead of the default headless) shows up as a genuine visible window on the \
             user's own desktop, not just an offscreen render. Only do this when actually \
             asked for or clearly useful (showing/demoing something live, not routine \
             scraping/automation, which should stay headless) — and close whatever you opened \
             once you're done with it rather than leaving it running. If the user hasn't \
             logged into a graphical session yet (e.g. right after a reboot), this will fail \
             with a plain \"cannot open display\" error rather than silently doing nothing.\n\n\
             Anything else — a package not in that list, a different language runtime, \
             whatever a task actually calls for — install it freely: `apt-get install x` \
             (package-manager commands get the privilege they need automatically, no `sudo` \
             required), `pip install x`, `npm install x`, `cargo add x`, `go get x`, and so \
             on. Don't hold back or ask first just because something isn't already present — \
             installing it is exactly what this tool is for. The one thing worth knowing: \
             anything installed this way (unlike the baked-in list above) only lasts for as \
             long as this container instance stays up, not permanently.\n\n\
             Dangerous — needs approval per command word (e.g. approving `python` once covers \
             any `python ...` call for the rest of the chat, with any arguments; `git` still \
             needs its own separate approval)."
                .to_string()
        } else {
            "Executes a shell command directly on the host machine this backend runs on, and \
             returns stdout, stderr, and exit code — each capped independently at 40,000 \
             characters (see `stdout_truncated`/`stderr_truncated`); a command whose real \
             output is bigger than that gets cut off, not silently sent in full. Be careful \
             with anything that can match inside large or single-line files (e.g. `grep -r` \
             across a directory with minified/bundled files) — `| head -N` only limits line \
             *count*, not size, so a handful of very long matched lines can still produce a \
             huge result; prefer a narrower search path or piping through something \
             byte-bounded (e.g. `| head -c 5000`) when the target directory's contents aren't \
             known ahead of time. This is real host access, not a sandbox — it can affect \
             actual host system state (installed packages, running services, anything a \
             normal shell command could touch), not just this backend's own files. \
             Dangerous — needs approval per command word (e.g. approving `python` once covers \
             any `python ...` call for the rest of the chat, with any arguments; `git` still \
             needs its own separate approval)."
                .to_string()
        }
    })
}

#[derive(Deserialize, tool_derive::ToolParams)]
struct ExecuteCommandArgs {
    #[tool(description = "The shell command to execute (e.g. 'ls -la', 'grep pattern file.txt').")]
    command: String,
    #[tool(description = "Directory to run the command from — absolute or relative, ~ expands to home. Defaults to this backend's own working directory if omitted (e.g. use this instead of prefixing the command with 'cd path &&').")]
    workdir: Option<String>,
}

pub struct ExecuteCommandTool;

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
const MAX_OUTPUT_CHARS: usize = 40_000;

fn truncate_output(content: String) -> (String, bool) {
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

#[derive(Serialize)]
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
    /// `true` when `stdout` is only the first `MAX_OUTPUT_CHARS` characters of the
    /// real output — see `truncate_output`.
    stdout_truncated: bool,
    /// Same as `stdout_truncated`, for `stderr`.
    stderr_truncated: bool,
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

#[async_trait]
impl Tool for ExecuteCommandTool {
    fn function_name(&self) -> &str {
        "os.execute_command"
    }

    fn description(&self) -> &str {
        description_text()
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ExecuteCommandArgs::tool_properties()
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: ResolvedScope,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: ExecuteCommandArgs = serde_json::from_value(data)?;

        let cmd_lower = args.command.to_lowercase();
        let has_destructive_command = statement_commands(&cmd_lower)
            .any(|word| word == "dd" || word == "mkfs" || word.starts_with("mkfs."));
        if cmd_lower.contains("rm -rf /") || cmd_lower.contains("> /dev/sd") || has_destructive_command {
            return Ok(ToolPermission::Denied {
                reason: "Blocked obviously destructive command pattern".to_string(),
                escalation: None,
            });
        }

        // Temporary, explicit opt-in for one unsupervised overnight run (2026-08-30) —
        // NOT a standing exception. Both env vars must be set AND match exactly
        // (trimmed) for this one specific call shape to skip approval; every other
        // call still requires a human, same as always. Remove both from
        // compose.yaml's backend environment (no rebuild needed, just recreate the
        // container) once the unattended stretch is over.
        let auto_cmd = std::env::var("EXECUTE_COMMAND_AUTO_APPROVE_CMD").ok();
        let auto_workdir = std::env::var("EXECUTE_COMMAND_AUTO_APPROVE_WORKDIR").ok();
        if let (Some(auto_cmd), Some(auto_workdir)) = (&auto_cmd, &auto_workdir) {
            if args.command.trim() == auto_cmd.trim()
                && args.workdir.as_deref().map(str::trim) == Some(auto_workdir.trim())
            {
                return Ok(ToolPermission::Allowed);
            }
        }

        let Some(base) = base_command(&args.command) else {
            return Ok(ToolPermission::Denied {
                reason: "couldn't find a command to run in an empty string".to_string(),
                escalation: None,
            });
        };

        let approved = scope
            .own
            .as_ref()
            .and_then(|s| s.get("approved_commands"))
            .and_then(|c| c.as_object())
            .is_some_and(|c| c.contains_key(base));

        if approved {
            return Ok(ToolPermission::Allowed);
        }

        let path = args.workdir.clone().or_else(|| parse_command_for_scope(&args.command));
        let reason = match &path {
            Some(path) => format!("Execute command requires approval (path context: {path})"),
            None => "Execute command requires approval".to_string(),
        };

        Ok(ToolPermission::Denied {
            reason,
            escalation: Some(ScopeGrant {
                scope: ResolvedScope {
                    own: Some(serde_json::json!({ "approved_commands": { base: true } })),
                    shared: std::collections::HashMap::new(),
                },
                ui_message: format!(
                    "Allow running `{base}` (with any arguments) for the rest of this chat? \
                     Only this one command, exactly as typed — not general shell access."
                ),
            }),
        })
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ExecuteCommandArgs = serde_json::from_value(data)?;

        // Prepends shell functions that redirect apt-get/apt/dpkg through a scoped
        // `sudo -n` (see `PACKAGE_MANAGER_SUDO_PREAMBLE`) rather than wrapping the
        // whole line in `sudo -n sh -c "..."` — the sudoers rule (see the Dockerfile)
        // only grants those three binaries directly, not `sh`, and it has to stay
        // that way: passwordless `sudo sh -c <anything>` would just be unrestricted
        // root, defeating the entire point of scoping this. Everything else in the
        // command (redirects, `;`, `&&`, non-package-manager commands) still runs as
        // this container's own unprivileged user, exactly as typed — only the
        // specific apt-get/apt/dpkg invocations resolve differently. `-n` (never
        // prompt) means if the sudoers rule somehow doesn't cover this, it fails
        // loudly with a clear stderr message instead of hanging on an unanswerable
        // password prompt.
        let effective_command = if running_in_docker() {
            format!("{PACKAGE_MANAGER_SUDO_PREAMBLE}{}", args.command)
        } else {
            args.command.clone()
        };
        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg(&effective_command);
        if let Some(workdir) = &args.workdir {
            command.current_dir(normalize(std::path::Path::new(workdir)));
        }

        let output = command
            .output()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't execute command: {e}")))?;

        let (stdout, stdout_truncated) = truncate_output(String::from_utf8_lossy(&output.stdout).to_string());
        let (stderr, stderr_truncated) = truncate_output(String::from_utf8_lossy(&output.stderr).to_string());

        Ok(serde_json::to_value(CommandOutput {
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
            stdout_truncated,
            stderr_truncated,
        })?)
    }
}
