//! The `tools` array of a chat request: the schema that tells a model what it may call. Ollama
//! and llama-server's OpenAI-compatible endpoint take the same shape, so every provider builds it
//! here.

use std::collections::BTreeMap;

use serde::{Serialize, Serializer, ser::SerializeMap};

use crate::tools::base::{PropertyInfo, PropertyType, Tool};

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
    /// the current date riding on the newest message only (`History`), this is the other half
    /// of what made Ollama/llama.cpp's prefix cache fail to match almost immediately on every turn.
    #[serde(serialize_with = "serialize_properties")]
    properties: BTreeMap<String, PropertyInfo>,
}

struct ToolProperty<'a> {
    kind: &'a PropertyType,
    description: Option<&'a str>,
}

impl Serialize for ToolProperty<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("type", &self.kind.to_string())?;
        if let Some(description) = self.description {
            map.serialize_entry("description", description)?;
        }
        match self.kind {
            PropertyType::Array { items } => {
                map.serialize_entry("items", &ToolProperty { kind: items, description: None })?;
            }
            PropertyType::Object { properties } if !properties.is_empty() => {
                let required: Vec<_> = properties.iter().filter(|p| p.required).map(|p| &p.name).collect();
                let children: BTreeMap<_, _> = properties.iter().map(|p| {
                    (&p.name, ToolProperty { kind: &p.property_type, description: Some(&p.description) })
                }).collect();
                map.serialize_entry("properties", &children)?;
                map.serialize_entry("required", &required)?;
            }
            _ => {}
        }
        map.end()
    }
}

fn serialize_properties<S: Serializer>(properties: &BTreeMap<String, PropertyInfo>, serializer: S) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(properties.len()))?;
    for (name, property) in properties {
        map.serialize_entry(name, &ToolProperty { kind: &property.property_type, description: Some(&property.description) })?;
    }
    map.end()
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
                .map(|property| (property.name.clone(), property))
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


