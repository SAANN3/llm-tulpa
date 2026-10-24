//! What a plugin says about itself and its tools (its settings in the manifest, its tools in `tools`) (see `plugin-sdk/PROTOCOL.md`),
//! checked before anything of it reaches the registry.

use serde::Deserialize;
use serde_json::Value;

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
    fn word(self) -> &'static str {
        match self {
            ParamType::String => "string",
            ParamType::Number => "number",
            ParamType::Integer => "integer",
            ParamType::Boolean => "boolean",
            ParamType::Array => "array",
            ParamType::Object => "object",
        }
    }

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

/// Which of the shared folder approvals a tool draws on: the same three every built-in tool that reads, writes or deletes
/// files uses, so a folder the user has approved for one is approved for the others
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    Read,
    Write,
    Delete,
}

/// A folder a tool works in, for the user's approval. Exactly one of `folder` and `path_arg` is set.
#[derive(Debug, Clone, Deserialize)]
pub struct Access {
    pub bucket: Bucket,
    /// A fixed folder; `{data_dir}` (the plugin's data folder) and `{user_id}` (the user whose chat the call is in) are
    /// replaced
    #[serde(default)]
    pub folder: Option<String>,
    /// The name of a string argument that holds the path the call works on (a file, or a folder)
    #[serde(default)]
    pub path_arg: Option<String>,
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
    /// The folders the tool reads, writes or deletes in: each is asked about the way a built-in tool's folder is, and a call
    /// runs only once every one is approved
    #[serde(default)]
    pub access: Vec<Access>,
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
        check_params(&format!("tool '{}'", tool.name), &tool.parameters)?;
        for access in &tool.access {
            match (&access.folder, &access.path_arg) {
                (Some(folder), None) => {
                    let rest = folder.replace("{data_dir}", "").replace("{user_id}", "");
                    if folder.is_empty() || rest.contains('{') || rest.contains('}') {
                        return Err(format!("tool '{}': the folder '{folder}' may only use {{data_dir}} and {{user_id}}", tool.name));
                    }
                }
                (None, Some(arg)) => {
                    if !tool.parameters.iter().any(|p| p.name == *arg && p.kind == ParamType::String) {
                        return Err(format!("tool '{}': path_arg '{arg}' is not a string argument of the tool", tool.name));
                    }
                }
                _ => return Err(format!("tool '{}': an access names either a folder or a path_arg", tool.name)),
            }
        }
    }
    Ok(())
}

/// Checks a list of parameters (a tool's arguments, a plugin's settings): each named once and every array with its element
/// type. `owner` says whose they are, for the message.
pub fn check_params(owner: &str, params: &[ParamSpec]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for param in params {
        if param.name.is_empty() || !seen.insert(param.name.as_str()) {
            return Err(format!("{owner}: '{}' is empty or listed twice", param.name));
        }
        if param.kind == ParamType::Array && param.items.is_none() {
            return Err(format!("{owner}: array '{}' needs its element type in \"items\"", param.name));
        }
    }
    Ok(())
}

fn fits(kind: ParamType, value: &Value) -> bool {
    match kind {
        ParamType::String => value.is_string(),
        ParamType::Number => value.is_number(),
        ParamType::Integer => value.is_i64() || value.is_u64(),
        ParamType::Boolean => value.is_boolean(),
        ParamType::Array => value.is_array(),
        ParamType::Object => value.is_object(),
    }
}

/// Whether `settings` is what the plugin's declared settings describe: an object that has every required one and gives each
/// declared one the type it names. A setting the schema does not list is left alone. The plugin only ever sees settings that
/// passed this, so it need not check types itself.
pub fn check_settings(schema: &[ParamSpec], settings: &Value) -> Result<(), String> {
    let object = settings.as_object().ok_or("the settings must be an object")?;
    for param in schema {
        match object.get(&param.name).filter(|value| !value.is_null()) {
            None if param.required => return Err(format!("setting '{}' is required", param.name)),
            None => {}
            Some(value) => {
                if !fits(param.kind, value) {
                    return Err(format!("setting '{}' must be of type {}", param.name, param.kind.word()));
                }
                if let (ParamType::Array, Some(items), Some(list)) = (param.kind, param.items, value.as_array()) {
                    if let Some(bad) = list.iter().position(|element| !fits(items, element)) {
                        return Err(format!("setting '{}': element {bad} must be of type {}", param.name, items.word()));
                    }
                }
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

    #[test]
    fn a_tools_folders_are_checked() {
        let ok = json!({"name": "a.b", "description": "d", "parameters": [{"name": "path", "type": "string", "description": "x"}],
            "access": [{"bucket": "write", "folder": "{data_dir}/{user_id}"}, {"bucket": "read", "path_arg": "path"}]});
        assert!(check_tools(&[tool(ok)]).is_ok());
        let cases = [
            (json!({"bucket": "write"}), "either a folder or a path_arg"),
            (json!({"bucket": "write", "folder": "/a", "path_arg": "path"}), "either a folder or a path_arg"),
            (json!({"bucket": "write", "folder": "{data_dir}/{other}"}), "{data_dir} and {user_id}"),
            (json!({"bucket": "read", "path_arg": "missing"}), "not a string argument"),
        ];
        for (access, expected) in cases {
            let t = tool(json!({"name": "a.b", "description": "d", "parameters": [{"name": "path", "type": "string", "description": "x"}], "access": [access]}));
            let err = check_tools(&[t]).unwrap_err();
            assert!(err.contains(expected), "{err:?} should mention {expected:?}");
        }
        assert!(serde_json::from_value::<ToolSpec>(json!({"name": "a.b", "description": "d", "access": [{"bucket": "execute", "folder": "/a"}]})).is_err());
    }

    #[test]
    fn settings_are_checked_against_what_the_manifest_declares() {
        let schema: Vec<ParamSpec> = serde_json::from_value(json!([
            {"name": "token", "type": "string", "description": "t", "required": true},
            {"name": "limit", "type": "integer", "description": "l"},
            {"name": "ids", "type": "array", "items": "integer", "description": "i"},
        ]))
        .unwrap();
        assert!(check_settings(&schema, &json!({"token": "x", "limit": 3, "ids": [1, 2], "other": true})).is_ok());
        assert!(check_settings(&schema, &json!({"token": "x", "limit": null})).is_ok());
        let cases = [
            (json!({}), "'token' is required"),
            (json!({"token": 5}), "'token' must be of type string"),
            (json!({"token": "x", "limit": 1.5}), "'limit' must be of type integer"),
            (json!({"token": "x", "ids": [1, "b"]}), "element 1"),
            (json!([1]), "must be an object"),
        ];
        for (settings, expected) in cases {
            let err = check_settings(&schema, &settings).unwrap_err();
            assert!(err.contains(expected), "{err:?} should mention {expected:?}");
        }
    }
}
