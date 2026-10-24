//! The registry's view of an external plugin. `ExternalPluginBuilder` answers the static questions (summary, settings, tab)
//! from the plugin's manifest and lives as long as the process; `ExternalPlugin` is one configured instance, whose hooks are
//! calls to the process, and hands the registry proxy tools whose calls go there too.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use serde_json::{Value, json};

use super::http;
use super::manifest::Manifest;
use super::process::PluginProcess;
use super::rpc::{BAD_ARGS, CallError, DEFAULT_CALL_TIMEOUT, METHOD_NOT_FOUND, NOT_FOUND};
use super::spec::{Access, Bucket, ToolSpec, check_settings, check_tools};
use crate::plugins::base::{Plugin, PluginBuilder, PluginError};
use crate::plugins::env::PluginEnv;
use crate::plugins::tab::{TabFile, TabInfo, TabLayout};
use crate::tools::base::{PropertyInfo, ResolvedScope, ScopeGrant, SharedBucket, Tool, ToolContext, ToolError, ToolPermission, ToolSerializationError};
use crate::tools::storage::{check_directory_scope, check_file_scope, normalize};

/// How long each hook may take. Every call to a plugin has a limit, so one that hangs costs the caller this much and no more.
const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(30);
const ENABLE_TIMEOUT: Duration = Duration::from_secs(30);
const DISABLE_TIMEOUT: Duration = Duration::from_secs(10);
/// A prompt section is asked for at the start of every turn, so a slow one would slow every chat
const PROMPT_TIMEOUT: Duration = Duration::from_secs(5);
const TAB_TIMEOUT: Duration = Duration::from_secs(30);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const USER_DELETED_TIMEOUT: Duration = Duration::from_secs(10);
/// Where uploaded files wait for the plugin to take them, inside its data folder
const STAGING_DIR: &str = ".staging";

/// Lives as long as the backend, like every builder; it owns the process, so a settings rebuild does not restart it.
pub struct ExternalPluginBuilder {
    manifest: Arc<Manifest>,
    /// The plugin's database steps, read from its migrations folder when it was loaded
    migrations: Vec<String>,
    process: Arc<PluginProcess>,
}

impl ExternalPluginBuilder {
    pub fn new(manifest: Manifest, migrations: Vec<String>, process: Arc<PluginProcess>) -> Self {
        Self { manifest: Arc::new(manifest), migrations, process }
    }
}

#[async_trait]
impl PluginBuilder for ExternalPluginBuilder {
    fn plugin_name(&self) -> &str {
        &self.manifest.name
    }

    fn plugin_subname(&self) -> &str {
        &self.manifest.subname
    }

    fn settings_schema(&self) -> Vec<PropertyInfo> {
        self.manifest.settings.iter().map(|setting| setting.property_info()).collect()
    }

    fn summary(&self) -> &str {
        &self.manifest.summary
    }

    fn help_message(&self) -> String {
        self.manifest.help_message.clone()
    }

    fn default_enabled(&self) -> bool {
        self.manifest.default_enabled
    }

    fn migrations(&self, _prefix: &str) -> Vec<String> {
        self.migrations.clone()
    }

    fn own_schema(&self) -> bool {
        self.manifest.database.is_some()
    }

    async fn shutdown(&self) {
        self.process.shutdown().await;
    }

    /// Settings that do not fit the manifest never reach the plugin. The plugin then gets them in `configure`, where it can
    /// refuse them for a reason only it knows (a token the service rejects), and lists its tools, which may depend on them.
    async fn build(&self, settings: Value, env: &PluginEnv) -> Result<Arc<dyn Plugin>, PluginError> {
        check_settings(&self.manifest.settings, &settings).map_err(PluginError::InvalidState)?;
        let link = Link { label: format!("{}/{}", self.manifest.name, self.manifest.subname), process: self.process.clone() };
        link.call_optional("configure", json!({"settings": settings}), CONFIGURE_TIMEOUT).await?;
        let tools = match link.call_optional("tools", json!({}), DEFAULT_CALL_TIMEOUT).await? {
            Some(listed) => serde_json::from_value::<Vec<ToolSpec>>(listed).map_err(|e| PluginError::FailedUnknown(format!("the plugin {} sent a tool list that is unusable: {e}", link.label)))?,
            None => vec![],
        };
        check_tools(&tools).map_err(|why| PluginError::FailedUnknown(format!("the plugin {}: {why}", link.label)))?;
        Ok(Arc::new(ExternalPlugin { manifest: self.manifest.clone(), link, settings, tools: Arc::new(tools), data_dir: env.data_dir.clone() }))
    }
}

/// The way to one plugin's process, with the answers turned into what the registry understands
#[derive(Clone)]
struct Link {
    label: String,
    process: Arc<PluginProcess>,
}

impl Link {
    /// A plugin may leave a method out; the SDKs answer all of them. `None` is "the plugin does not have it".
    async fn call_optional(&self, method: &str, params: Value, timeout: Duration) -> Result<Option<Value>, PluginError> {
        match self.process.call(method, params, timeout).await {
            Ok(result) => Ok(Some(result)),
            Err(CallError::Rpc(e)) if e.code == METHOD_NOT_FOUND => Ok(None),
            Err(e) => Err(self.error(e, timeout)),
        }
    }

    /// What the plugin's own words become: a refusal of the request is the caller's mistake (400), a missing thing is a 404,
    /// the rest is the plugin failing
    fn error(&self, e: CallError, timeout: Duration) -> PluginError {
        match e {
            CallError::Rpc(e) if e.code == BAD_ARGS => PluginError::InvalidState(e.message),
            CallError::Rpc(e) if e.code == NOT_FOUND => PluginError::NotFound(e.message),
            CallError::Rpc(e) => PluginError::FailedUnknown(format!("the plugin {}: {}", self.label, e.message)),
            CallError::Timeout => PluginError::FailedUnknown(format!("the plugin {} did not answer within {} s", self.label, timeout.as_secs())),
            CallError::Closed => PluginError::FailedUnknown(format!("the plugin {} is not running", self.label)),
        }
    }

    async fn layout(&self, method: &str, params: Value, timeout: Duration) -> Result<TabLayout, PluginError> {
        let answer = self
            .call_optional(method, params, timeout)
            .await?
            .ok_or_else(|| PluginError::NotFound(format!("the plugin {} has no tab", self.label)))?;
        serde_json::from_value(answer).map_err(|e| PluginError::FailedUnknown(format!("the plugin {} sent a tab layout that is unusable: {e}", self.label)))
    }
}

pub struct ExternalPlugin {
    manifest: Arc<Manifest>,
    link: Link,
    settings: Value,
    tools: Arc<Vec<ToolSpec>>,
    data_dir: PathBuf,
}

#[async_trait]
impl Plugin for ExternalPlugin {
    fn plugin_name(&self) -> &str {
        &self.manifest.name
    }

    fn plugin_subname(&self) -> &str {
        &self.manifest.subname
    }

    fn settings_value(&self) -> Value {
        self.settings.clone()
    }

    fn api_router(&self) -> Router {
        http::router(self.link.label.clone(), self.link.process.clone())
    }

    async fn on_enabled(&self) -> Result<(), PluginError> {
        self.link.call_optional("enable", json!({}), ENABLE_TIMEOUT).await?;
        Ok(())
    }

    /// A plugin that cannot be told to stop (it crashed, it hangs) is switched off all the same: the owner must be able to turn
    /// off what is misbehaving, and what is not running has nothing left to stop.
    async fn on_disabled(&self) -> Result<(), PluginError> {
        if let Err(e) = self.link.call_optional("disable", json!({}), DISABLE_TIMEOUT).await {
            tracing::warn!("{e}");
        }
        Ok(())
    }

    fn tools(&self) -> Vec<Box<dyn Tool>> {
        self.tools
            .iter()
            .map(|spec| {
                Box::new(ExternalTool { spec: spec.clone(), plugin: self.link.label.clone(), data_dir: self.data_dir.clone(), process: self.link.process.clone() }) as Box<dyn Tool>
            })
            .collect()
    }

    /// A plugin that fails to answer has no section this turn: the chat goes on without it rather than waiting on it
    async fn prompt_section(&self, user_id: i64, tools: bool) -> Option<String> {
        match self.link.call_optional("prompt_section", json!({"user_id": user_id, "tools": tools}), PROMPT_TIMEOUT).await {
            Ok(Some(Value::String(text))) => Some(text),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!("{e}");
                None
            }
        }
    }

    async fn on_user_deleted(&self, user_id: i64) {
        if let Err(e) = self.link.call_optional("user_deleted", json!({"user_id": user_id}), USER_DELETED_TIMEOUT).await {
            tracing::warn!("{e}");
        }
    }

    fn settings_tab(&self) -> Option<TabInfo> {
        self.manifest.tab.as_ref().map(|tab| TabInfo { id: tab.id.clone(), title: tab.title.clone() })
    }

    async fn tab_layout(&self, user_id: i64) -> Result<TabLayout, PluginError> {
        self.link.layout("tab_layout", json!({"user_id": user_id}), TAB_TIMEOUT).await
    }

    async fn tab_action(&self, user_id: i64, action: &str, args: Value) -> Result<TabLayout, PluginError> {
        self.link.layout("tab_action", json!({"user_id": user_id, "action": action, "args": args}), TAB_TIMEOUT).await
    }

    /// The files are written under the plugin's data folder and handed over as paths (a file never travels inside a protocol
    /// line); the plugin moves what it keeps, and the staging folder is removed when the call ends.
    async fn tab_upload(&self, user_id: i64, action: &str, row: Option<String>, files: Vec<TabFile>) -> Result<TabLayout, PluginError> {
        let staging = self.data_dir.join(STAGING_DIR).join(format!("{:016x}", rand::random::<u64>()));
        let staged = stage(&staging, files).await;
        let result = match staged {
            Ok(staged) => self.link.layout("tab_upload", json!({"user_id": user_id, "action": action, "row": row, "files": staged}), UPLOAD_TIMEOUT).await,
            Err(e) => Err(e),
        };
        // Best effort: what is left behind is the plugin's own folder, and the next upload gets a folder of its own
        if let Err(e) = tokio::fs::remove_dir_all(&staging).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!("plugin {}: could not remove {}: {e}", self.link.label, staging.display());
            }
        }
        result
    }
}

/// Writes the uploaded files into `staging`, each at the path the browser gave it (a dropped folder keeps its shape), and
/// lists them as `{path, file}`. A path that would leave `staging` is refused.
async fn stage(staging: &Path, files: Vec<TabFile>) -> Result<Vec<Value>, PluginError> {
    let io = |what: &str, e: std::io::Error| PluginError::FailedUnknown(format!("{what}: {e}"));
    let mut staged = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        let relative = staged_path(&file.path).ok_or_else(|| PluginError::InvalidState(format!("'{}' is not a usable file path", file.path)))?;
        let target = staging.join(&relative);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| io("could not stage an upload", e))?;
        }
        // The same path twice in one upload keeps both, rather than one silently replacing the other
        let target = if tokio::fs::try_exists(&target).await.unwrap_or(false) { target.with_file_name(format!("{index}-{}", relative.file_name().and_then(|n| n.to_str()).unwrap_or("file"))) } else { target };
        tokio::fs::write(&target, &file.bytes).await.map_err(|e| io("could not stage an upload", e))?;
        staged.push(json!({"path": file.path, "file": target}));
    }
    Ok(staged)
}

/// The relative path a browser's file name stands for: plain names only, so it cannot name anything outside the folder it
/// is written in (a leading slash is dropped). `None` for a path with a `..` or with no name at all.
fn staged_path(path: &str) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for part in path.replace('\\', "/").split('/') {
        match Path::new(part).components().next() {
            None | Some(Component::CurDir) => {}
            Some(Component::Normal(_)) if Path::new(part).components().count() == 1 => clean.push(part),
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

/// A tool of an external plugin: the model sees it like any other, and running it is a `call_tool` to the process
struct ExternalTool {
    spec: ToolSpec,
    plugin: String,
    data_dir: PathBuf,
    process: Arc<PluginProcess>,
}

fn shared_bucket(bucket: Bucket) -> SharedBucket {
    match bucket {
        Bucket::Read => SharedBucket::StorageRead,
        Bucket::Write => SharedBucket::StorageWrite,
        Bucket::Delete => SharedBucket::StorageDelete,
    }
}

impl ExternalTool {
    fn uses(&self, bucket: Bucket) -> bool {
        self.spec.access.iter().any(|access| access.bucket == bucket)
    }

    /// What the user is asked about for one access, or `None` when the call has nothing there to ask about
    fn check_access(&self, access: &Access, user_id: i64, data: &Value, scope: &ResolvedScope) -> Option<ToolPermission> {
        let bucket = shared_bucket(access.bucket);
        let granted = scope.shared.get(&bucket);
        if let Some(folder) = &access.folder {
            let folder = folder.replace("{data_dir}", &self.data_dir.to_string_lossy()).replace("{user_id}", &user_id.to_string());
            return Some(check_directory_scope(&folder, bucket, granted));
        }
        let arg = access.path_arg.as_deref()?;
        match data.get(arg) {
            None | Some(Value::Null) => None,
            Some(Value::String(path)) => {
                // A folder is what is asked about when the path is one; for a file it is the folder the file is in
                Some(if normalize(Path::new(path)).is_dir() { check_directory_scope(path, bucket, granted) } else { check_file_scope(path, bucket, granted) })
            }
            Some(_) => Some(ToolPermission::Denied { reason: format!("'{arg}' must be a path, as text"), escalation: None }),
        }
    }
}

#[async_trait]
impl Tool for ExternalTool {
    fn function_name(&self) -> &str {
        &self.spec.name
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        self.spec.parameters.iter().map(|p| p.property_info()).collect()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        use SharedBucket::{StorageDelete as D, StorageRead as R, StorageWrite as W};
        match (self.uses(Bucket::Read), self.uses(Bucket::Write), self.uses(Bucket::Delete)) {
            (false, false, false) => &[],
            (true, false, false) => &[R],
            (false, true, false) => &[W],
            (false, false, true) => &[D],
            (true, true, false) => &[R, W],
            (true, false, true) => &[R, D],
            (false, true, true) => &[W, D],
            (true, true, true) => &[R, W, D],
        }
    }

    /// The tool's own bucket holds the approval of `confirm`; it is kept next to the shared ones only when the tool asks
    fn uses_own_bucket(&self) -> bool {
        self.spec.confirm || self.spec.access.is_empty()
    }

    /// The folders in `access` are asked about one at a time, in the order the spec lists them, each the way a built-in tool's
    /// folder is (a grant for a folder covers every tool that uses it). Then, for a tool that asked for it (`confirm`), the
    /// user's approval before it first runs in a chat, which holds for that chat. Only the plugin knows what else a call does,
    /// so nothing finer is offered.
    fn is_dangerous_for(&self, user_id: i64, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        for access in &self.spec.access {
            if let Some(permission @ ToolPermission::Denied { .. }) = self.check_access(access, user_id, &data, &scope) {
                return Ok(permission);
            }
        }
        self.is_dangerous(data, scope)
    }

    fn is_dangerous(&self, _data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let approved = scope.own.as_ref().and_then(|own| own.get("approved")).and_then(Value::as_bool) == Some(true);
        if !self.spec.confirm || approved {
            return Ok(ToolPermission::Allowed);
        }
        Ok(ToolPermission::Denied {
            reason: format!("{} (a tool of the external plugin {}) requires approval", self.spec.name, self.plugin),
            escalation: Some(ScopeGrant {
                scope: ResolvedScope { own: Some(json!({"approved": true})), shared: HashMap::new() },
                ui_message: format!("Allow the external plugin {} to run its tool '{}' for the rest of this chat?", self.plugin, self.spec.name),
            }),
        })
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let params = json!({
            "tool": self.spec.name,
            "args": data,
            "ctx": {"user_id": ctx.user_id, "chat_id": ctx.chat_id, "model": ctx.model, "provider": ctx.provider},
        });
        let timeout = self.spec.timeout_s.map(Duration::from_secs).unwrap_or(DEFAULT_CALL_TIMEOUT);
        self.process.call("call_tool", params, timeout).await.map_err(|e| {
            ToolError::FailedUnknown(match e {
                CallError::Rpc(e) => e.message,
                CallError::Timeout => format!("the plugin {} did not answer within {} s", self.plugin, timeout.as_secs()),
                CallError::Closed => format!("the plugin {} is not running", self.plugin),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_upload_path_stays_inside_its_folder() {
        assert_eq!(staged_path("a.md"), Some(PathBuf::from("a.md")));
        assert_eq!(staged_path("skill/docs/a.md"), Some(PathBuf::from("skill/docs/a.md")));
        assert_eq!(staged_path("./skill//a.md"), Some(PathBuf::from("skill/a.md")));
        assert_eq!(staged_path("skill\\a.md"), Some(PathBuf::from("skill/a.md")));
        assert_eq!(staged_path("/etc/passwd"), Some(PathBuf::from("etc/passwd")), "a leading slash is dropped, the path stays inside");
        for bad in ["", "../a", "a/../../b", "a/.."] {
            assert_eq!(staged_path(bad), None, "{bad}");
        }
    }

    #[tokio::test]
    async fn files_are_staged_under_their_paths_and_a_repeated_path_keeps_both() {
        let dir = std::env::temp_dir().join(format!("tulpa-stage-{}", std::process::id()));
        let files = vec![
            TabFile { path: "s/a.md".into(), bytes: b"one".to_vec() },
            TabFile { path: "s/a.md".into(), bytes: b"two".to_vec() },
        ];
        let staged = stage(&dir, files).await.unwrap();
        let first = staged[0]["file"].as_str().unwrap();
        let second = staged[1]["file"].as_str().unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(first).unwrap(), b"one");
        assert_eq!(std::fs::read(second).unwrap(), b"two");
        assert!(stage(&dir, vec![TabFile { path: "../x".into(), bytes: vec![] }]).await.is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
