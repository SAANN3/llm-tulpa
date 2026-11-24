use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

/// How many options a question may list: fewer than two is not a choice, more than six
/// is too many for the overlay.
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 6;

/// Reserved for the extra free-text row in the UI; a model-supplied option may not use it.
pub const CUSTOM_OPTION_ID: &str = "custom";

pub struct AskQuestionTool;

impl AskQuestionTool {
    pub const NAME: &'static str = "ui.ask_question";
}

#[derive(Deserialize, ToolParams)]
struct AskQuestionArgs {
    #[tool(description = "The question to show the user, in their language.")]
    question: String,
    #[tool(
        description = "Two to six options. Each has a short id (not 'custom') and the label the user sees.",
        nested
    )]
    options: Vec<AskOption>,
    #[tool(description = "If true (the default), the user can type an answer that is not one of the options.")]
    allow_custom: Option<bool>,
}

#[derive(Deserialize, ToolParams)]
struct AskOption {
    #[tool(description = "A short identifier for this option, unique in this call. Not 'custom'.")]
    id: String,
    #[tool(description = "The text on the button.")]
    label: String,
}

impl AskQuestionArgs {
    fn check(&self) -> Result<(), String> {
        let question = self.question.trim();
        if question.is_empty() {
            return Err("the question is empty".to_string());
        }
        if self.options.len() < MIN_OPTIONS || self.options.len() > MAX_OPTIONS {
            return Err(format!("a question needs {MIN_OPTIONS} to {MAX_OPTIONS} options"));
        }
        let mut seen = std::collections::HashSet::new();
        for option in &self.options {
            let id = option.id.trim();
            let label = option.label.trim();
            if id.is_empty() || label.is_empty() {
                return Err("each option needs an id and a label".to_string());
            }
            if id == CUSTOM_OPTION_ID {
                return Err(format!("'{CUSTOM_OPTION_ID}' is reserved for a typed answer"));
            }
            if !seen.insert(id.to_string()) {
                return Err(format!("option id '{id}' is used twice"));
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct AskQuestionOut {
    shown: bool,
    question: String,
    /// What the model should do with this result. The pick itself is the user's next message.
    note: &'static str,
}

#[async_trait]
impl Tool for AskQuestionTool {
    fn function_name(&self) -> &str {
        Self::NAME
    }

    fn description(&self) -> &str {
        "Shows the user a question with two to six options (allow_custom, default true, adds a \
         free-text row; the id 'custom' is reserved for it). Use it when a choice only they can \
         make would change what you do next. Don't use it to confirm a step you can already take, \
         to list the options in the reply as well, or to ask something you could look up. This \
         call returns as soon as the question is on screen. Their answer is their next message, \
         not this result — when you need it before doing more, end the turn."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        AskQuestionArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: AskQuestionArgs = serde_json::from_value(data)?;
        args.check().map_err(ToolError::FailedUnknown)?;
        Ok(serde_json::to_value(AskQuestionOut {
            shown: true,
            question: args.question,
            note: "Shown in the chat. Their answer arrives as the next user message. If you need it before doing more, end the turn.",
        })
        .expect("AskQuestionOut is a bool, a string and a static note"))
    }
}
