//! The user's list of MCP servers. Adding one here is the same connection the model makes with `mcp.add_server`.

use serde_json::Value;

use super::config::ConnectionConfig;
use super::runtime::Runtime;
use super::store::ServerStore;
use crate::plugins::base::PluginError;
use crate::plugins::tab::{Block, Row, RowButton, RowSwitch, Section, TabLayout};

pub async fn layout(store: &ServerStore, runtime: &Runtime, user_id: i64, message: Option<String>) -> Result<TabLayout, PluginError> {
    let rows = store.list(user_id).await.map_err(|e| PluginError::FailedUnknown(e.to_string()))?;
    let mut items = Vec::new();
    for server in rows {
        let tools = runtime.tool_count(user_id, &server.name).await;
        let subtitle = match (&server.last_error, tools) {
            (Some(error), _) => error.clone(),
            (None, Some(n)) => format!("{} tools", n),
            (None, None) if server.enabled => "connecting".into(),
            (None, None) => "off".into(),
        };
        items.push(Row {
            id: server.name.clone(),
            title: server.name.clone(),
            subtitle: Some(format!("{} · {}", server.config.target(), subtitle)),
            badge: Some(server.config.transport().into()),
            switch: Some(RowSwitch { action: "enable".into(), on: server.enabled }),
            buttons: vec![RowButton { action: "remove".into(), label: "Remove".into(), confirm: Some("Remove this server?".into()), upload: false }],
        });
    }
    Ok(TabLayout {
        sections: vec![
            Section {
                title: Some("Servers".into()),
                blocks: vec![Block::Items { empty: "No servers yet.".into(), rows: items }],
            },
            Section {
                title: Some("Add".into()),
                blocks: vec![
                    Block::Text {
                        text: "One line: name, then http or stdio, then the url or the command. Example: firefox stdio npx -y @agent/firefox".into(),
                        dim: true,
                    },
                    Block::Input {
                        action: "add".into(),
                        label: "Server".into(),
                        value: String::new(),
                        placeholder: Some("name http https://…  or  name stdio command".into()),
                        help: None,
                        password: false,
                        submit_label: Some("Add".into()),
                        args: Default::default(),
                    },
                ],
            },
        ],
        message,
        popup: None,
    })
}

pub async fn action(store: &ServerStore, runtime: &Runtime, user_id: i64, action: &str, args: Value) -> Result<TabLayout, PluginError> {
    let message = match action {
        "add" => Some(add(runtime, user_id, args.get("value").and_then(|v| v.as_str()).unwrap_or("")).await?),
        "enable" => {
            let name = args.get("row").and_then(|v| v.as_str()).unwrap_or("");
            let on = args.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            runtime.set_enabled(user_id, name, on).await.map_err(PluginError::FailedUnknown)?;
            None
        }
        "remove" => {
            let name = args.get("row").and_then(|v| v.as_str()).unwrap_or("");
            runtime.remove(user_id, name).await.map_err(PluginError::FailedUnknown)?;
            None
        }
        other => return Err(PluginError::InvalidState(format!("unknown action '{other}'"))),
    };
    let _ = store;
    layout(store, runtime, user_id, message).await
}

/// `name http <url>` or `name stdio <command…>`.
async fn add(runtime: &Runtime, user_id: i64, line: &str) -> Result<String, PluginError> {
    let mut parts = line.split_whitespace();
    let name = parts.next().ok_or_else(|| PluginError::InvalidState("say the server name, the transport, and the target".into()))?;
    let transport = parts.next().ok_or_else(|| PluginError::InvalidState("say http or stdio, then the target".into()))?;
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() {
        return Err(PluginError::InvalidState("say the url or the command".into()));
    }
    let config = match transport {
        "http" => ConnectionConfig::Http { url: rest.join(" "), headers: Default::default() },
        "stdio" => ConnectionConfig::Stdio {
            command: rest[0].to_string(),
            args: rest[1..].iter().map(|s| (*s).to_string()).collect(),
            env: Default::default(),
            cwd: None,
        },
        _ => return Err(PluginError::InvalidState("the transport is http or stdio".into())),
    };
    let count = runtime.add(user_id, name, config).await.map_err(PluginError::FailedUnknown)?;
    Ok(format!("{name} connected, {count} tools"))
}
