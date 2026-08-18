use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolError, ToolParams, ToolPermission, ToolSerializationError};

use super::{check_file_scope, normalize, replace_in_file};

pub struct ReplaceStrTool;

#[derive(Deserialize, ToolParams)]
struct ReplaceStrArgs {
    #[tool(description = "Absolute or relative path to an existing file.")]
    path: String,
    #[tool(
        description = "An exact, unique snippet of the file's current text to replace. Must \
                        match exactly once; the call fails rather than guessing if it's \
                        missing or appears more than once — include more surrounding context \
                        to disambiguate, or read the file first if the exact current text \
                        isn't already certain."
    )]
    old_string: String,
    #[tool(
        description = "The text that takes `old_string`'s place. This is ONLY the new text \
                        for that exact spot — sized to match whatever's actually different, \
                        never the whole file, regardless of what kind of file it is. If \
                        `new_string` ends up close in size to the whole file, that's a sign \
                        the whole file was passed by mistake instead of just the part that \
                        changed."
    )]
    new_string: String,
}

#[derive(Serialize)]
struct ReplaceStrOut {
    path: String,
    bytes_written: usize,
}

#[async_trait]
impl Tool for ReplaceStrTool {
    fn function_name(&self) -> &str {
        "storage.replace_str"
    }

    fn description(&self) -> &str {
        "Replaces one exact, unique occurrence of `old_string` with `new_string` in an \
         existing file — a precise edit instead of rewriting the whole file. This is the \
         preferred way to change part of a file that already exists; use storage.write_file \
         instead only for a brand new file or a genuine full rewrite."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReplaceStrArgs::tool_properties()
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: Option<Value>,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: ReplaceStrArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, scope))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: ReplaceStrArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        let bytes_written = replace_in_file(&path, &args.old_string, &args.new_string).await?;

        Ok(serde_json::to_value(ReplaceStrOut {
            path: path.to_string_lossy().to_string(),
            bytes_written,
        })?)
    }
}
