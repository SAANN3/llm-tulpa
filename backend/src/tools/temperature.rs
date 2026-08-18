use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::Sleep;
use tool_derive::ToolParams;

use super::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams};

pub struct TemperatureTool;

#[derive(Deserialize, ToolParams)]
struct TemperatureArgs {
    #[tool(description = "The city or place to get the temperature for.")]
    location: String,
}

#[derive(Serialize)]
struct TemperatureOut {
    location: String,
    temperature_celsius: f64,
}

#[async_trait]
impl Tool for TemperatureTool {
    fn function_name(&self) -> &str {
        "get_temperature"
    }

    fn description(&self) -> &str {
        "Get the current temperature for a given location."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        TemperatureArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: TemperatureArgs = serde_json::from_value(data)?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let out = TemperatureOut {
            location: args.location,
            temperature_celsius: 21.5,
        };

        Ok(serde_json::to_value(out)?)
    }
}
