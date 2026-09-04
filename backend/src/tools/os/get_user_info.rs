use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;

use crate::tools::base::{PropertyInfo, Tool, ToolError, ToolParams};

#[derive(Deserialize, tool_derive::ToolParams)]
struct GetUserInfoArgs {}

#[derive(Serialize)]
struct UserInfoOut {
    username: String,
    home_dir: String,
    current_exe: Option<String>,
}

pub struct GetUserInfoTool;

#[async_trait]
impl Tool for GetUserInfoTool {
    fn function_name(&self) -> &str {
        "os.get_user_info"
    }

    fn description(&self) -> &str {
        "Returns information about the current user running this backend: username, home directory path, and process executable path (if available)."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        GetUserInfoArgs::tool_properties()
    }

    async fn call_untyped(&self, _data: Value) -> Result<Value, ToolError> {
        let username = env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());

        let home_dir = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/unknown".to_string());

        let current_exe = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().to_string());

        Ok(serde_json::to_value(UserInfoOut {
            username,
            home_dir,
            current_exe,
        })?)
    }
}
