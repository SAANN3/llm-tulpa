use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::tools::base::{PropertyInfo, Tool, ToolContext, ToolError};

pub struct ListModelsTool;

#[derive(Serialize)]
struct ModelInfo {
    id: String,
    label: String,
    description: String,
    languages: String,
    installed: bool,
}

#[derive(Serialize)]
struct ListModelsOut {
    models: Vec<ModelInfo>,
}

#[async_trait]
impl Tool for ListModelsTool {
    fn function_name(&self) -> &str {
        "stt.list_models"
    }

    fn description(&self) -> &str {
        "Lists the speech-to-text models available for the stt.transcribe tool, each with its \
         id, a short description and the languages it understands, and whether it is installed. \
         Only an installed model can be used for transcription: call this first and pass the id \
         of one marked installed, as stt.transcribe's model."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    async fn call_untyped(&self, _data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let models = ctx
            .stt
            .status()
            .models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                label: m.label,
                description: m.description,
                languages: m.languages,
                installed: m.installed,
            })
            .collect();
        Ok(serde_json::to_value(ListModelsOut { models })?)
    }
}
