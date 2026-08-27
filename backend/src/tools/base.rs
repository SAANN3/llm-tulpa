use std::fmt;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;


/// Anything that can be exposed to the model as a callable tool. One impl per tool.
/// The model never sees this trait directly — its methods feed into the JSON tool
/// schema Ollama expects (name/description/parameters), and `call_untyped` is what
/// actually runs once the model asks for this tool by name.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Ollama's tool schema has a `type` field alongside `function`. In practice every
    /// tool we've seen (and every one Ollama's docs show) is `"function"` — there's no
    /// other documented variant — so this defaults to `Function` and only exists to
    /// mirror the upstream schema rather than to be meaningfully overridden.
    fn tool_type(&self) -> ToolType {
        ToolType::Function
    }

    /// The identifier the model uses to call this tool and that we use to route an
    /// incoming tool call back to this impl. Not a human-facing label — think
    /// `"get_temperature"`, not `"Temperature Tool"`.
    fn function_name(&self) -> &str;

    /// Explains what the tool does and, implicitly, when the model should call it.
    /// This is documentation for the model, not for humans reading the source — the
    /// model decides whether/when to call the tool based on this text, so vague or
    /// missing detail here directly causes missed or wrong tool calls.
    fn description(&self) -> &str;

    /// The parameters this tool needs, in the shape Ollama's tool schema expects
    /// (name + type + description per parameter). Drives both what schema we advertise
    /// to the model and what `call_untyped` expects to find in `data`.
    fn required_properties(&self) -> Vec<PropertyInfo>;

    /// Just the names of the properties actually marked `required`, for call sites that
    /// only need to check presence/validate keys without caring about types or
    /// descriptions. `required_properties()` still returns every property (required or
    /// not) — the model needs to see optional ones too, just not listed as mandatory.
    fn required_param_names(&self) -> Vec<String> {
        self.required_properties()
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.clone())
            .collect()
    }

    /// Decides whether this specific call is allowed to run. `data` is the raw call
    /// arguments the model produced (same shape `call_untyped` gets), so a tool can
    /// judge danger from the actual values (e.g. which path, which command) rather than
    /// treating every call as uniformly dangerous. `scope` is whatever this tool
    /// previously got granted for the current chat (via `PermissionStore`), if
    /// anything — opaque to everything except this impl, which is the only thing that
    /// knows how to interpret its own `ScopeGrant::scope`. Defaults to always-`Allowed`
    /// since most tools (like a lookup) are safe to run unattended.
    ///
    /// The `Err` case exists purely for "I can't even read `data` well enough to judge
    /// it" (malformed/unexpected shape) — see `ToolSerializationError`'s doc comment
    /// for why that's a separate, single-purpose type rather than something an impl
    /// could be tempted to reach for to express a permission decision. A call this
    /// tool has actually understood and refuses is always `Ok(ToolPermission::Denied
    /// { .. })`, never an `Err`.
    fn is_dangerous(&self, data: Value, scope: Option<Value>) -> Result<ToolPermission, ToolSerializationError> {
        let _ = (data, scope);
        Ok(ToolPermission::Allowed)
    }

    /// Runs the tool. Takes the raw JSON arguments object the model produced (matched
    /// against `required_properties`) rather than a typed struct, since the trait can't
    /// know each impl's argument shape — each impl deserializes `data` into its own
    /// typed args internally. Returns raw JSON back for the same reason: results vary
    /// per tool (a string, a number, a nested object), and the caller re-serializes
    /// whatever comes back into the `tool` message sent back to the model.
    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError>;
}

/// Mirrors the `type` field of Ollama's tool schema. See `Tool::tool_type`.
pub enum ToolType {
    Function,
}

impl fmt::Display for ToolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolType::Function => write!(f, "function"),
        }
    }
}

/// One parameter in a tool's schema — name, JSON type, and a description the model
/// reads to decide what value to fill in. Also doubles as a plugin settings field's
/// schema (see `plugins::base::PluginBuilder::settings_schema`) — same shape, same
/// derive, reused rather than duplicated for the frontend's settings-form renderer.
#[derive(Serialize, ToSchema)]
pub struct PropertyInfo {
    pub name: String,
    pub property_type: PropertyType,
    pub description: String,
    /// Whether the model must supply this argument. `#[derive(ToolParams)]` sets this
    /// from the field's own type — `false` for `Option<T>`, `true` otherwise — so a
    /// tool's args struct is the one place this is decided.
    pub required: bool,
}

/// JSON Schema primitive types, restricted to what Ollama's tool-calling schema
/// actually accepts as a parameter type.
#[derive(Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PropertyType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}

impl fmt::Display for PropertyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PropertyType::String => "string",
            PropertyType::Number => "number",
            PropertyType::Integer => "integer",
            PropertyType::Boolean => "boolean",
            PropertyType::Array => "array",
            PropertyType::Object => "object",
        };
        write!(f, "{s}")
    }
}

/// Why a tool call failed. `Deserialization` covers the model producing arguments that
/// don't match what `call_untyped` expected (wrong type, missing field); `FailedUnknown`
/// is a catch-all for tool-specific failures until impls need more specific variants.
/// The `Display` output here is what ends up in the `tool` message sent back to the
/// model, so the model can see *that* its call failed, not just get silence.
pub enum ToolError {
    Deserialization(serde_json::Error),
    FailedUnknown(String),
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::Deserialization(e) => write!(f, "Failed to run tool, err = {e}"),
            ToolError::FailedUnknown(reason) => write!(f, "Failed to run tool, err = {reason}"),
        }
    }
}

impl From<serde_json::Error> for ToolError {
    fn from(e: serde_json::Error) -> Self {
        ToolError::Deserialization(e)
    }
}

/// The result of `Tool::is_dangerous` — whether a specific call is allowed to run
/// given whatever scope has already been granted.
pub enum ToolPermission {
    /// Args fall inside an existing grant. Run it.
    Allowed,
    /// Needs approval. `escalation` is present when there's a broader grant worth
    /// offering ("allow all reads under ~/folder"), absent when the action should only
    /// ever be approved one call at a time.
    Denied {
        reason: String,
        escalation: Option<ScopeGrant>,
    },
}

/// A scope a tool is offering to have granted, plus what to tell the user about it.
pub struct ScopeGrant {
    /// Opaque to everything except the tool that produced it.
    pub scope: Value,
    /// What granting this actually permits. English for now.
    pub ui_message: String,
}

/// The only way `Tool::is_dangerous` can fail: it couldn't understand `data`/`scope`
/// well enough to make a call at all (malformed shape, wrong types). Deliberately a
/// single-variant wrapper around a deserialization error and nothing else — if an
/// impl is ever tempted to construct one of these to express "this call isn't
/// permitted," that's the wrong tool for the job; `Ok(ToolPermission::Denied { .. })`
/// is how a call that was understood but refused gets expressed. Keeping this type
/// incapable of carrying anything but a real `serde_json::Error` is what makes that
/// misuse awkward to write by accident.
pub struct ToolSerializationError(pub serde_json::Error);

impl fmt::Display for ToolSerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "couldn't read tool call data: {}", self.0)
    }
}

impl From<serde_json::Error> for ToolSerializationError {
    fn from(e: serde_json::Error) -> Self {
        ToolSerializationError(e)
    }
}

/// Implemented by `#[derive(ToolParams)]` (see the `tool_derive` crate) on a tool's args
/// struct — generates `tool_properties()` from each field's type and `#[tool(description
/// = "...")]` attribute, so `Tool::required_properties` can delegate to
/// `<Args>::tool_properties()` instead of a hand-written `Vec<PropertyInfo>` literal.
pub trait ToolParams {
    fn tool_properties() -> Vec<PropertyInfo>;
}
