use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams};

#[derive(Deserialize, tool_derive::ToolParams)]
struct EnvReadArgs {
    #[tool(description = "The environment variable name to read. Omit to read all variables (returns a JSON object).")]
    key: Option<String>,
}

#[derive(Serialize)]
struct EnvReadOut {
    value: String,
}

pub struct EnvReadTool;

#[async_trait]
impl Tool for EnvReadTool {
    fn function_name(&self) -> &str {
        "os.env_read"
    }

    fn description(&self) -> &str {
        "Reads environment variables of this backend process. With no arguments returns all \
         env vars as a JSON object. With a key argument returns just that variable's value."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        EnvReadArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: EnvReadArgs = serde_json::from_value(data)?;

        if let Some(key) = &args.key {
            let value = std::env::var(key).unwrap_or_default();
            Ok(serde_json::to_value(&EnvReadOut { value })?)
        } else {
            // Return all env vars as a JSON object
            let mut map: serde_json::Map<String, Value> = serde_json::Map::new();
            for (k, v) in std::env::vars() {
                map.insert(k.clone(), serde_json::Value::String(v));
            }
            Ok(serde_json::to_value(map)?)
        }
    }
}
