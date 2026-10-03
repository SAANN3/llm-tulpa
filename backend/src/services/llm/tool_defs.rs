//! The `tools` array of a chat request: the schema that tells a model what it may call. Ollama
//! and llama-server's OpenAI-compatible endpoint take the same shape, so every provider builds it
//! here.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::tools::base::Tool;

/// One entry of the `tools` array in a chat request — the schema the server forwards to the
/// model so it knows what's callable. Ollama and llama-server's OpenAI-compatible endpoint take
/// the same shape. See `tool_definitions`.
#[derive(Serialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    definition_type: String,
    function: ToolFunctionDefinition,
}

#[derive(Serialize)]
pub struct ToolFunctionDefinition {
    name: String,
    description: String,
    parameters: ToolParameters,
}

#[derive(Serialize)]
pub struct ToolParameters {
    #[serde(rename = "type")]
    parameters_type: String,
    required: Vec<String>,
    /// `BTreeMap`, not `HashMap` — this gets rebuilt fresh on every `chat` call, and
    /// `HashMap`'s randomized per-instance hasher seed means its (and therefore
    /// `serde_json`'s) key order isn't stable across two builds of the same content.
    /// The chat template renders this into the literal prompt text, so an unstable
    /// order here changes the prompt's bytes on every single call — right alongside
    /// `Agent::advance`'s own prompt-cache fix, this is the other half of what made
    /// Ollama/llama.cpp's prefix cache fail to match almost immediately on every turn.
    properties: BTreeMap<String, ToolProperty>,
}

#[derive(Serialize)]
pub struct ToolProperty {
    #[serde(rename = "type")]
    property_type: String,
    description: String,
}


/// Maps the given tools into the JSON schema a chat request's `tools` field takes
/// (`{"type": "function", "function": {name, description, parameters: {...}}}`).
pub fn tool_definitions(tools: &[&dyn Tool]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .map(|tool| {
            let properties = tool
                .required_properties()
                .into_iter()
                .map(|property| {
                    (
                        property.name,
                        ToolProperty {
                            property_type: property.property_type.to_string(),
                            description: property.description,
                        },
                    )
                })
                .collect();

            ToolDefinition {
                definition_type: tool.tool_type().to_string(),
                function: ToolFunctionDefinition {
                    name: tool.function_name().to_string(),
                    description: tool.description().to_string(),
                    parameters: ToolParameters {
                        parameters_type: "object".to_string(),
                        required: tool.required_param_names(),
                        properties,
                    },
                },
            }
        })
        .collect()
}


