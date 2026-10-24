//! What a plugin may ask of the backend, besides logging: its database and the agent. One of these goes to the plugin's process
//! (`process.rs` answers the plugin's requests with it).

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::agent::PluginAgent;
use super::db::{PluginDb, Sql};
use super::rpc::{BAD_ARGS, INTERNAL, METHOD_NOT_FOUND, RpcError};

/// `None` for what the plugin did not ask for: a database needs `[database]` in the manifest. Everything is `None` for the one
/// start an install makes to see that the program runs.
#[derive(Default)]
pub struct HostServices {
    pub db: Option<Arc<PluginDb>>,
    pub agent: Option<Arc<PluginAgent>>,
}

impl HostServices {
    /// Answers `db.*` and `agent.*`; anything else is not the host's
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let bad = |e: serde_json::Error| RpcError::new(BAD_ARGS, format!("{method}: {e}"));
        let failed = |why: String| RpcError::new(INTERNAL, why);
        match method {
            "agent.reply" => self.agent.as_ref().ok_or_else(|| RpcError::new(BAD_ARGS, "the agent is not available to this plugin"))?.reply(params).await,
            "db.query" | "db.execute" | "db.batch" => {
                let db = self.db.as_ref().ok_or_else(|| RpcError::new(BAD_ARGS, "this plugin has no database: add [database] to plugin.toml"))?;
                match method {
                    "db.query" => db.query(serde_json::from_value::<Sql>(params).map_err(bad)?).await.map_err(failed),
                    "db.execute" => Ok(json!(db.execute(serde_json::from_value::<Sql>(params).map_err(bad)?).await.map_err(failed)?)),
                    _ => {
                        #[derive(Deserialize)]
                        struct Batch {
                            statements: Vec<Sql>,
                        }
                        let batch: Batch = serde_json::from_value(params).map_err(bad)?;
                        Ok(json!(db.batch(batch.statements).await.map_err(failed)?))
                    }
                }
            }
            _ => Err(RpcError::new(METHOD_NOT_FOUND, format!("the host has no method {method}"))),
        }
    }
}
