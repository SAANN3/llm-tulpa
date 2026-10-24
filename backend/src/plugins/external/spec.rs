//! What a plugin says about itself and its tools, as it arrives in `tools` (see `plugin-sdk/PROTOCOL.md`),
//! checked before anything of it reaches the registry.

use serde::Deserialize;

use crate::tools::base::{PropertyInfo, PropertyType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}

impl ParamType {
    fn property_type(self) -> PropertyType {
        match self {
            ParamType::String => PropertyType::String,
            ParamType::Number => PropertyType::Number,
            ParamType::Integer => PropertyType::Integer,
            ParamType::Boolean => PropertyType::Boolean,
            ParamType::Array => PropertyType::Array,
            ParamType::Object => PropertyType::Object,
        }
    }
}

/// One argument of a tool
#[derive(Debug, Clone, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ParamType,
    pub description: String,
    #[serde(default)]
    pub required: bool,
    /// For an array, the type of its elements
    #[serde(default)]
    pub items: Option<ParamType>,
}

impl ParamSpec {
    pub fn property_info(&self) -> PropertyInfo {
        PropertyInfo {
            name: self.name.clone(),
            property_type: self.kind.property_type(),
            description: self.description.clone(),
            required: self.required,
            items: self.items.map(ParamType::property_type),
        }
    }
}

/// One tool a plugin offers the model
#[derive(Debug, Clone, Deserialize)]
pub struct ToolSpec {
    /// `domain.action`
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<ParamSpec>,
    /// Ask the user before every call
    #[serde(default)]
    pub confirm: bool,
    /// How long a call may take; the default applies when absent
    #[serde(default)]
    pub timeout_s: Option<u64>,
}

fn is_word(part: &str) -> bool {
    part.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Whether `name` has the shape of every tool name, `domain.action`
pub fn valid_tool_name(name: &str) -> bool {
    name.split_once('.').is_some_and(|(domain, action)| is_word(domain) && is_word(action))
}

/// Checks a list of tool specs as a whole: each name well formed, none twice, each argument named once, every array with
/// its element type (a chat template that writes the schema out needs it), and a description on each tool.
pub fn check_tools(tools: &[ToolSpec]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for tool in tools {
        if !valid_tool_name(&tool.name) {
            return Err(format!("tool '{}': a tool name looks like domain.action, in lowercase letters, digits and underscores", tool.name));
        }
        if !seen.insert(tool.name.as_str()) {
            return Err(format!("tool '{}' is listed twice", tool.name));
        }
        if tool.description.trim().is_empty() {
            return Err(format!("tool '{}' has no description; the model decides when to call a tool from it", tool.name));
        }
        let mut args = std::collections::HashSet::new();
        for param in &tool.parameters {
            if param.name.is_empty() || !args.insert(param.name.as_str()) {
                return Err(format!("tool '{}': argument '{}' is empty or listed twice", tool.name, param.name));
            }
            if param.kind == ParamType::Array && param.items.is_none() {
                return Err(format!("tool '{}': array argument '{}' needs its element type in \"items\"", tool.name, param.name));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(v: serde_json::Value) -> ToolSpec {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn tool_names_are_domain_dot_action() {
        assert!(valid_tool_name("hello.greet"));
        assert!(valid_tool_name("screen_shot.take_2"));
        for bad in ["greet", "Hello.greet", "hello.", ".greet", "hello.greet.twice", "hello greet", "1a.b"] {
            assert!(!valid_tool_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_tool_spec_reads_its_arguments_and_defaults() {
        let t = tool(json!({"name": "a.b", "description": "d", "parameters": [{"name": "n", "type": "array", "items": "integer", "description": "x", "required": true}]}));
        assert!(!t.confirm && t.timeout_s.is_none());
        let info = t.parameters[0].property_info();
        assert!(info.required && matches!(info.property_type, PropertyType::Array) && matches!(info.items, Some(PropertyType::Integer)));
        assert!(check_tools(&[t]).is_ok());
    }

    #[test]
    fn a_list_with_a_mistake_is_refused_with_the_tool_named() {
        let ok = json!({"name": "a.b", "description": "d"});
        let cases = [
            (vec![ok.clone(), ok.clone()], "twice"),
            (vec![json!({"name": "ab", "description": "d"})], "domain.action"),
            (vec![json!({"name": "a.b", "description": " "})], "no description"),
            (vec![json!({"name": "a.b", "description": "d", "parameters": [{"name": "l", "type": "array", "description": "x"}]})], "element type"),
            (vec![json!({"name": "a.b", "description": "d", "parameters": [{"name": "l", "type": "string", "description": "x"}, {"name": "l", "type": "string", "description": "x"}]})], "listed twice"),
        ];
        for (list, expected) in cases {
            let specs: Vec<ToolSpec> = list.into_iter().map(tool).collect();
            let err = check_tools(&specs).unwrap_err();
            assert!(err.contains(expected), "{err:?} should mention {expected:?}");
        }
    }
}
