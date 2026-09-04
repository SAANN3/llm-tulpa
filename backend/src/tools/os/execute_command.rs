use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, ScopeGrant, Tool, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
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
            "Executes a shell command and returns stdout, stderr, and exit code. Runs inside \
             this backend's own container, not directly on the host machine — it can freely \
             install/use anything inside that container (e.g. `pip install x`, `cargo check`) \
             without touching the real host's package state, and can read/write any path the \
             storage.* tools can reach (the container shares that same filesystem access). It \
             cannot do host-system-wide things: no host package manager, no host systemd/ \
             service control, no host-level `docker` commands. Dangerous — needs approval per \
             command word (e.g. approving `python` once covers any `python ...` call for the \
             rest of the chat, with any arguments; `git` still needs its own separate approval)."
                .to_string()
        } else {
            "Executes a shell command directly on the host machine this backend runs on, and \
             returns stdout, stderr, and exit code. This is real host access, not a sandbox — \
             it can affect actual host system state (installed packages, running services, \
             anything a normal shell command could touch), not just this backend's own files. \
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

#[derive(Serialize)]
struct CommandOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
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
        if cmd_lower.contains("rm -rf /") || cmd_lower.contains("mkfs") ||
           cmd_lower.contains("> /dev/sd") || cmd_lower.contains("dd ") {
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

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: ExecuteCommandArgs = serde_json::from_value(data)?;

        let mut command = tokio::process::Command::new("sh");
        command.arg("-c").arg(&args.command);
        if let Some(workdir) = &args.workdir {
            command.current_dir(normalize(std::path::Path::new(workdir)));
        }

        let output = command
            .output()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't execute command: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(serde_json::to_value(CommandOutput {
            stdout,
            stderr,
            exit_code: output.status.code().unwrap_or(-1),
        })?)
    }
}
