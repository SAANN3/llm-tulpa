use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct ReturnAgentTool;

impl ReturnAgentTool {
    // A const, not a literal in `function_name`, so `subagent::available_to` can name it without repeating the string.
    pub const NAME: &'static str = "llm.return_agent";
}

#[derive(Deserialize, ToolParams)]
struct ReturnAgentArgs {
    #[tool(
        description = "The result, written so it stands on its own: the answer itself plus the concrete details \
                        needed to use it (paths, names, values, how sure you are). Leave out the search trail."
    )]
    output: String,
}

#[async_trait]
impl Tool for ReturnAgentTool {
    fn function_name(&self) -> &str {
        Self::NAME
    }

    fn description(&self) -> &str {
        "For a sub-agent only: hands the result of your task back to the assistant that gave it to you, \
         and ends this run. Call it once, as the only call in that step, when the task is done — or when \
         it can't be finished, saying what you found and what stopped you. `output` is the only thing that \
         assistant receives: it has seen none of this conversation, so make it stand alone, and keep it \
         short — a very long output is cut off."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReturnAgentArgs::tool_properties()
    }

    // The run driver reads this tool's result as the sub-agent's answer, so it must be exactly `output`.
    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReturnAgentArgs = serde_json::from_value(data)?;
        if args.output.trim().is_empty() {
            return Err(ToolError::FailedUnknown("the output is empty — write the result to hand back".to_string()));
        }
        Ok(Value::String(args.output))
    }
}
