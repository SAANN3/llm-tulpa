//! The tool descriptions a plugin sends in `tools`.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}

/// One argument of a tool. Required unless `optional` is called.
#[derive(Clone, Debug, Serialize)]
pub struct Param {
    name: String,
    #[serde(rename = "type")]
    kind: ParamType,
    description: String,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<ParamType>,
}

impl Param {
    pub fn new(name: &str, kind: ParamType, description: &str) -> Self {
        Self { name: name.into(), kind, description: description.into(), required: true, items: None }
    }

    pub fn string(name: &str, description: &str) -> Self {
        Self::new(name, ParamType::String, description)
    }

    pub fn integer(name: &str, description: &str) -> Self {
        Self::new(name, ParamType::Integer, description)
    }

    pub fn number(name: &str, description: &str) -> Self {
        Self::new(name, ParamType::Number, description)
    }

    pub fn boolean(name: &str, description: &str) -> Self {
        Self::new(name, ParamType::Boolean, description)
    }

    /// An array of `items`
    pub fn array(name: &str, items: ParamType, description: &str) -> Self {
        Self { items: Some(items), ..Self::new(name, ParamType::Array, description) }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
}

/// Which of the user's folder approvals a tool draws on. The same three approvals every file tool of the backend uses, so a
/// folder the user has approved for one is approved for the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    Read,
    Write,
    Delete,
}

#[derive(Clone, Debug, Serialize)]
struct Access {
    bucket: Bucket,
    #[serde(skip_serializing_if = "Option::is_none")]
    folder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path_arg: Option<String>,
}

/// One tool the model can call. The description is what the model reads to decide when to call it.
#[derive(Clone, Debug, Serialize)]
pub struct ToolSpec {
    name: String,
    description: String,
    parameters: Vec<Param>,
    confirm: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    access: Vec<Access>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_s: Option<u64>,
}

impl ToolSpec {
    /// `name` is `domain.action`, in lowercase letters, digits and underscores
    pub fn new(name: &str, description: &str) -> Self {
        Self { name: name.into(), description: description.into(), parameters: vec![], confirm: false, access: vec![], timeout_s: None }
    }

    pub fn param(mut self, param: Param) -> Self {
        self.parameters.push(param);
        self
    }

    /// The user is asked before every call
    pub fn confirm(mut self) -> Self {
        self.confirm = true;
        self
    }

    /// The tool works in a folder: the user is asked to approve `bucket` access to it before a call runs, as for any file tool.
    /// `folder` may use `{data_dir}` (the plugin's data folder) and `{user_id}` (the user whose chat the call is in).
    pub fn access_folder(mut self, bucket: Bucket, folder: &str) -> Self {
        self.access.push(Access { bucket, folder: Some(folder.into()), path_arg: None });
        self
    }

    /// The tool works on the path that its string argument `arg` holds (a file, or a folder): the user is asked to approve
    /// `bucket` access to it, for a file to the folder it is in.
    pub fn access_path_arg(mut self, bucket: Bucket, arg: &str) -> Self {
        self.access.push(Access { bucket, folder: None, path_arg: Some(arg.into()) });
        self
    }

    /// How long a call may take before the backend gives up on it (30 s when not set)
    pub fn timeout_s(mut self, seconds: u64) -> Self {
        self.timeout_s = Some(seconds);
        self
    }
}
