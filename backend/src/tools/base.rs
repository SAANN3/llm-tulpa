use std::collections::HashMap;
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
    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
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

    /// Shared, cross-tool permission buckets this tool's scope draws from, in addition
    /// to (or instead of) its own name-keyed bucket. Empty by default — most tools just
    /// use their own bucket, unshared, exactly as `is_dangerous` always has. A tool
    /// listing one or more here gets checked/granted against those buckets instead (see
    /// `uses_own_bucket`), so e.g. every `storage.*` read tool can share one folder
    /// grant instead of each needing its own separate approval for the same folder.
    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[]
    }

    /// Whether this tool also has its own private, name-keyed bucket for facts no
    /// shared bucket covers (e.g. `web.download_file`'s allowed hosts, alongside the
    /// folder it shares with `storage.write_file`). Defaults to `true` exactly when
    /// `shared_buckets()` is empty — a tool declaring no shared bucket obviously still
    /// needs somewhere to store its scope.
    fn uses_own_bucket(&self) -> bool {
        self.shared_buckets().is_empty()
    }
}

/// One of the shared, cross-tool permission buckets a tool can draw its scope from —
/// see `Tool::shared_buckets`. Variants are fully independent levels: granting
/// `StorageWrite` does NOT imply `StorageRead` (deliberate — least-privilege over
/// convenience). A tool needing more than one declares every bucket it needs; nothing
/// here infers one from another.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SharedBucket {
    StorageRead,
    StorageWrite,
    StorageDelete,
}

impl SharedBucket {
    /// The literal `tool_permissions.tool_name` value this bucket is actually
    /// stored/looked-up under. `GLOBAL.`-prefixed and upper-cased so it can never
    /// collide with a real tool's `function_name()` — every real tool name is a
    /// lowercase `domain.snake_case` string, never this shape. Purely a backend/DB
    /// implementation detail: the frontend only ever sees and sends real tool names,
    /// never one of these.
    pub fn db_key(self) -> &'static str {
        match self {
            SharedBucket::StorageRead => "GLOBAL.STORAGE_READ",
            SharedBucket::StorageWrite => "GLOBAL.STORAGE_WRITE",
            SharedBucket::StorageDelete => "GLOBAL.STORAGE_DELETE",
        }
    }

    /// The JSON key this bucket's grant is stored under within its own row (a map of
    /// granted folders — see `tools::storage::check_scope`). Named once here, on the
    /// enum itself, so every storage-domain bucket sharing this shape (they all do
    /// today) stays in sync automatically, and a future rename touches only this match.
    pub fn json_key(self) -> &'static str {
        match self {
            SharedBucket::StorageRead | SharedBucket::StorageWrite | SharedBucket::StorageDelete => "folders",
        }
    }
}

/// A tool's resolved permission scope for one call — its own private bucket (if it has
/// one) plus each shared bucket it declared, kept separate so e.g. read-granted folders
/// are never confused with write-granted ones even though both use the same JSON key
/// (`SharedBucket::json_key`) internally. Passed into `Tool::is_dangerous` in place of a
/// single flat value; built by `Agent::stored_scope`/`Agent::split_scope`.
#[derive(Default, Clone)]
pub struct ResolvedScope {
    pub own: Option<Value>,
    pub shared: HashMap<SharedBucket, Value>,
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
/// `scope` is the same `ResolvedScope` shape `is_dangerous` reads its current scope
/// from — own facts kept apart from each shared bucket's, by construction, rather than
/// flattened into one JSON object a reader would have to guess back apart. That's what
/// makes a tool needing two buckets that happen to store their grant under the same
/// `SharedBucket::json_key()` (none do today) structurally collision-proof: each bucket
/// gets its own map entry, never a shared top-level key two buckets could stomp on.
pub struct ScopeGrant {
    pub scope: ResolvedScope,
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
