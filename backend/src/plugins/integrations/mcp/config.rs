//! How one server is reached. Stored as JSON on the user's row.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `http` talks to a URL. `stdio` starts a local program and speaks MCP on its stdin/stdout.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectionConfig {
    Http {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        cwd: Option<String>,
    },
}

impl ConnectionConfig {
    pub fn transport(&self) -> &'static str {
        match self {
            ConnectionConfig::Http { .. } => "http",
            ConnectionConfig::Stdio { .. } => "stdio",
        }
    }

    /// A short line for the settings list.
    pub fn target(&self) -> String {
        match self {
            ConnectionConfig::Http { url, .. } => url.clone(),
            ConnectionConfig::Stdio { command, args, .. } => {
                if args.is_empty() { command.clone() } else { format!("{command} {}", args.join(" ")) }
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            ConnectionConfig::Http { url, headers } => {
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err("an http server needs a url starting with http:// or https://".into());
                }
                for (name, value) in headers {
                    if name.is_empty() || name.contains(['\r', '\n', ':']) || value.contains(['\r', '\n']) {
                        return Err(format!("header '{name}' is not a single-line name and value"));
                    }
                }
            }
            ConnectionConfig::Stdio { command, args, env, cwd } => {
                if command.trim().is_empty() || command.contains('\0') {
                    return Err("a stdio server needs a command".into());
                }
                if args.iter().any(|a| a.contains('\0')) || env.keys().any(|k| k.is_empty() || k.contains(['=', '\0'])) {
                    return Err("stdio args and env names cannot be empty or contain a null".into());
                }
                if cwd.as_ref().is_some_and(|c| c.is_empty() || c.contains('\0')) {
                    return Err("cwd must be a path".into());
                }
            }
        }
        Ok(())
    }
}

/// A server name is the domain of every tool it publishes (`firefox.list_tabs`).
pub fn valid_server_name(name: &str) -> bool {
    name != "mcp"
        && name.len() <= 32
        && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
