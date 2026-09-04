use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, ScopeGrant, Tool, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
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

    let key_upper = key.to_uppercase();
    !blocked.iter().any(|&b| key_upper == b || key_upper.starts_with(b))
}

#[async_trait]
impl Tool for EnvWriteTool {
    fn function_name(&self) -> &str {
        "os.env_write"
    }

    fn description(&self) -> &str {
        "Writes environment variables. Modifies the runtime environment of this backend \
         process. Dangerous — needs approval per variable name (e.g. approving 'FOO' once \
         covers any future value written to FOO for the rest of the chat; a different \
         variable still needs its own separate approval)."
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

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: EnvWriteArgs = serde_json::from_value(data)?;

        // Get previous value if it exists
        let previous_value = std::env::var(&args.key).ok();

        // Set the new value
        unsafe {
            std::env::set_var(&args.key, &args.value);
        }

        Ok(serde_json::to_value(EnvWriteOut {
            success: true,
            previous_value,
        })?)
    }
}
