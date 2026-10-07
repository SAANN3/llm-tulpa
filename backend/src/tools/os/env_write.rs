use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, ScopeGrant, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};

#[derive(Deserialize, tool_derive::ToolParams)]
struct EnvWriteArgs {
    #[tool(description = "The environment variable name to set.")]
    key: String,
    #[tool(description = "The value to set for the variable.")]
    value: String,
}

pub struct EnvWriteTool;

#[derive(Serialize)]
struct EnvWriteOut {
    success: bool,
    previous_value: Option<String>,
}

fn is_safe_env_var(key: &str) -> bool {
    // Block writing to obviously sensitive or critical env vars
    let blocked = [
        "PATH", "LD_LIBRARY_PATH", "PYTHONPATH", "HOME",
        "DISPLAY", "XDG_RUNTIME_DIR", "WAYLAND_DISPLAY",
        "DB_PASSWORD", "DATABASE_URL", "SECRET_KEY",
        "TOKEN", "API_KEY", "PRIVATE_KEY",
    ];

    // Variables that make the shell or the dynamic linker run code of their choosing in every later command
    let runs_code = ["LD_", "DYLD_", "BASH_ENV", "BASH_FUNC_"];
    let runs_code_exact = ["ENV", "PROMPT_COMMAND", "SHELLOPTS", "BASHOPTS", "IFS", "PS4"];

    let key_upper = key.to_uppercase();
    !blocked.iter().any(|&b| key_upper == b || key_upper.starts_with(b))
        && !runs_code.iter().any(|&prefix| key_upper.starts_with(prefix))
        && !runs_code_exact.contains(&key_upper.as_str())
}

#[async_trait]
impl Tool for EnvWriteTool {
    fn function_name(&self) -> &str {
        "os.env_write"
    }

    fn description(&self) -> &str {
        "Sets an environment variable for the commands this backend runs from now on \
         (os.execute_command, os.start_job), until the backend restarts. Dangerous — needs \
         approval per variable name (e.g. approving 'FOO' once covers any future value \
         written to FOO for the rest of the chat; a different variable still needs its own \
         separate approval)."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        EnvWriteArgs::tool_properties()
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: ResolvedScope,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: EnvWriteArgs = serde_json::from_value(data)?;

        // Block obviously dangerous env vars
        if !is_safe_env_var(&args.key) {
            return Ok(ToolPermission::Denied {
                reason: format!("Blocked write to potentially sensitive env var '{}'", args.key),
                escalation: None,
            });
        }

        let approved = scope
            .own
            .as_ref()
            .and_then(|s| s.get("approved_keys"))
            .and_then(|k| k.as_object())
            .is_some_and(|k| k.contains_key(&args.key));

        if approved {
            return Ok(ToolPermission::Allowed);
        }

        Ok(ToolPermission::Denied {
            reason: "Write environment variable requires approval".to_string(),
            escalation: Some(ScopeGrant {
                scope: ResolvedScope {
                    own: Some(serde_json::json!({ "approved_keys": { args.key.clone(): true } })),
                    shared: std::collections::HashMap::new(),
                },
                ui_message: format!(
                    "Allow writing to the environment variable '{}' (any value) for the rest of \
                     this chat? Only this one variable — not env var writes in general.",
                    args.key
                ),
            }),
        })
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: EnvWriteArgs = serde_json::from_value(data)?;

        let previous_value = crate::services::process::set_command_env(&args.key, &args.value);

        Ok(serde_json::to_value(EnvWriteOut {
            success: true,
            previous_value,
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_that_inject_code_into_later_commands_are_refused() {
        for key in ["LD_PRELOAD", "ld_audit", "BASH_ENV", "ENV", "PROMPT_COMMAND", "DYLD_INSERT_LIBRARIES", "PATH", "API_KEY_X"] {
            assert!(!is_safe_env_var(key), "{key} should be refused");
        }
        for key in ["FOO", "NODE_ENV", "ENVIRONMENT", "RUST_LOG"] {
            assert!(is_safe_env_var(key), "{key} should be allowed");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_written_variable_reaches_later_commands_without_touching_the_backend() {
        let previous = crate::services::process::set_command_env("TULPA_ENV_WRITE_TEST", "42");
        assert_eq!(previous, None);
        assert!(std::env::var("TULPA_ENV_WRITE_TEST").is_err(), "the backend's own environment stays as it was");
        let output = crate::services::process::shell_command("echo \"$TULPA_ENV_WRITE_TEST\"", None).output().await.unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
        assert_eq!(crate::services::process::command_env_var("TULPA_ENV_WRITE_TEST").as_deref(), Some("42"));
        assert_eq!(crate::services::process::set_command_env("TULPA_ENV_WRITE_TEST", "43").as_deref(), Some("42"));
    }
}
