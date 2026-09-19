use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::shell::{check_command_permission, effective_command, running_in_docker, truncate_output};
use crate::services::process;
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError, ToolParams,
    ToolPermission, ToolSerializationError,
};
use crate::tools::storage::normalize;

/// Appended to both variants of the description — what the model can rely on when a
/// command starts something long-running.
const BACKGROUND_AND_TIMEOUT_NOTE: &str = "For anything that keeps running or takes a long time — a \
    dev server, a watcher, a big build or install — use os.start_job instead of putting it in the \
    background here: it gives the job an id and a log you can read with os.job_output, and you're \
    notified in the chat when it finishes. A command can still be started with `&` here — this \
    returns as soon as the shell itself exits, without waiting for it — but the background process's \
    output isn't captured anywhere you can get back to. A command still running in the foreground \
    after 10 minutes is killed along with everything it started, and whatever it printed by then is \
    returned.";

fn description_text() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let base = if running_in_docker() {
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
        };
        format!("{base}\n\n{BACKGROUND_AND_TIMEOUT_NOTE}")
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

/// How long a command may keep the shell running before it's killed. Only a backstop
/// against a foreground command that never exits (a server started without `&`,
/// `tail -f`, a prompt waiting on input that will never come) wedging the whole chat —
/// generous enough for a real build or install, which can take several minutes. Anything
/// legitimately longer belongs in the background (`cmd > log 2>&1 &`) with the log polled.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Scratch file a command's stdout or stderr is redirected into, instead of a pipe.
/// With a pipe, reading the output means waiting for EOF, and EOF only comes once
/// *every* process holding the write end has exited — including a daemon the command
/// backgrounded with `&`, which inherits it. A file has no EOF to wait for: the shell's
/// own exit is the only thing waited on, and a backgrounded process keeps writing to its
/// (by then deleted) file undisturbed — unlike a closed pipe, which would kill it with
/// a broken-pipe error the next time it logged anything.
struct OutputFile {
    path: PathBuf,
}

impl OutputFile {
    fn create(stream: &str) -> std::io::Result<(Self, std::fs::File)> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "llm-tulpa-cmd-{}-{nanos}-{}.{stream}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
        ));

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);

        let file = options.open(&path)?;
        Ok((Self { path }, file))
    }

    async fn read(&self) -> String {
        let bytes = tokio::fs::read(&self.path).await.unwrap_or_default();
        String::from_utf8_lossy(&bytes).to_string()
    }
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
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

/// Runs `effective_command` under `sh -c` and collects its output — see `OutputFile` for
/// why that's files rather than pipes, and `COMMAND_TIMEOUT` for what `timeout` is a
/// backstop against. Takes the timeout as a parameter only so it's exercisable without
/// waiting out the real one.
async fn run_shell(
    command_line: &str,
    workdir: Option<&str>,
    timeout: Duration,
) -> Result<CommandOutput, ToolError> {
    let scratch_error = |e: std::io::Error| ToolError::FailedUnknown(format!("couldn't set up command output: {e}"));
    let (stdout_file, stdout_handle) = OutputFile::create("out").map_err(scratch_error)?;
    let (stderr_file, stderr_handle) = OutputFile::create("err").map_err(scratch_error)?;

    let mut command = process::shell_command(command_line, workdir.map(|dir| normalize(std::path::Path::new(dir))));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_handle))
        .stderr(Stdio::from(stderr_handle));
    process::detach(&mut command);

    let mut child = command
        .spawn()
        .map_err(|e| ToolError::FailedUnknown(format!("couldn't execute command: {e}")))?;
    drop(command);

    let wait_error = |e: std::io::Error| ToolError::FailedUnknown(format!("couldn't wait for command: {e}"));
    let (status, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => (status.map_err(wait_error)?, false),
        Err(_) => {
            if let Some(pid) = child.id() {
                process::kill_process_tree(pid).await;
            }
            let _ = child.kill().await;
            (child.wait().await.map_err(wait_error)?, true)
        }
    };

    let (stdout, stdout_truncated) = truncate_output(stdout_file.read().await);
    let (mut stderr, stderr_truncated) = truncate_output(stderr_file.read().await);
    if timed_out {
        stderr.push_str(&format!(
            "\n[command killed: still running after {}s. Output above is whatever it \
             produced by then. Run anything this long in the background instead \
             (`cmd > /tmp/log 2>&1 &`) and check the log.]",
            timeout.as_secs()
        ));
    }

    Ok(CommandOutput {
        stdout,
        stderr,
        exit_code: status.code().unwrap_or(-1),
        stdout_truncated,
        stderr_truncated,
    })
}

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

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::ShellCommands]
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: ResolvedScope,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: ExecuteCommandArgs = serde_json::from_value(data)?;

        // Temporary, explicit opt-in for one unsupervised overnight run (2026-08-30) —
        // NOT a standing exception. Both env vars must be set AND match exactly
        // (trimmed) for this one specific call shape to skip approval; every other
        // call still requires a human, same as always. Remove both from
        // compose.yaml's backend environment (no rebuild needed, just recreate the
        // container) once the unattended stretch is over.
        let auto_cmd = std::env::var("EXECUTE_COMMAND_AUTO_APPROVE_CMD").ok();
        let auto_workdir = std::env::var("EXECUTE_COMMAND_AUTO_APPROVE_WORKDIR").ok();
        let auto_approved = match (&auto_cmd, &auto_workdir) {
            (Some(auto_cmd), Some(auto_workdir)) => {
                args.command.trim() == auto_cmd.trim()
                    && args.workdir.as_deref().map(str::trim) == Some(auto_workdir.trim())
            }
            _ => false,
        };

        Ok(check_command_permission(
            &args.command,
            args.workdir.as_deref(),
            scope.shared.get(&SharedBucket::ShellCommands),
            auto_approved,
        ))
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ExecuteCommandArgs = serde_json::from_value(data)?;

        Ok(serde_json::to_value(
            run_shell(&effective_command(&args.command), args.workdir.as_deref(), COMMAND_TIMEOUT).await?,
        )?)
    }
}
