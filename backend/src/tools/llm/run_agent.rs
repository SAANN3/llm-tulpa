use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

pub struct RunAgentTool;

impl RunAgentTool {
    // A const, not a literal in `function_name`, so `subagent::available_to` can name it without repeating the string.
    pub const NAME: &'static str = "llm.run_agent";
}

#[derive(Deserialize, ToolParams)]
struct RunAgentArgs {
    #[tool(
        description = "The whole task, written so someone who has seen none of this conversation can do it: \
                        the goal, the paths, names or ids involved, where to look, and what form of answer you \
                        want back."
    )]
    prompt: String,
}

#[derive(Serialize)]
struct RunAgentOut {
    job_id: i64,
    sub_chat_id: i64,
    note: &'static str,
}

#[async_trait]
impl Tool for RunAgentTool {
    fn function_name(&self) -> &str {
        Self::NAME
    }

    fn description(&self) -> &str {
        "Hands a task to a sub-agent: a separate assistant with a fresh conversation of its own, the \
         same tools you have (except this one and the background-job tools) and the permissions this \
         chat has been given so far, which works on the task and reports back one result. Its work never \
         enters this conversation, so use it for a task that would take many tool calls whose \
         intermediate output you won't need afterwards — finding something across a large codebase, \
         researching a question over many pages — instead of filling your own context with it. Not for a \
         quick lookup you can do in a call or two yourself. It saves context, not time: sub-agents run \
         one at a time on the same model, so the task finishes no faster than if you did it. It runs in \
         the background — this call returns at once with a job id, and a message with the sub-agent's \
         result appears in the chat when it finishes — so either carry on with other work or end your turn \
         saying what you're waiting for. os.job_kill stops it. The sub-agent sees nothing of this \
         conversation, so the prompt has to contain everything it needs. It can't ask you or the user \
         anything, and it can't get a tool call approved: whatever isn't already permitted is refused \
         for it."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        RunAgentArgs::tool_properties()
    }

    // No permission gating: the sub-agent starts with this chat's grants and can be given no more.
    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: RunAgentArgs = serde_json::from_value(data)?;
        if args.prompt.trim().is_empty() {
            return Err(ToolError::FailedUnknown("the prompt is empty — say what the sub-agent should do".to_string()));
        }

        let started = ctx.subagents.start(ctx.chat_id, args.prompt).await?;

        Ok(serde_json::to_value(RunAgentOut {
            job_id: started.job_id,
            sub_chat_id: started.chat_id,
            note: "Started, and running in the background. You don't have its result yet, so don't answer as if \
                   you did — tell the user it's underway, or carry on with other work. Its result will appear \
                   in this chat as a message when it finishes.",
        })?)
    }
}
