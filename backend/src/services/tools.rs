use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::RwLock;
use serde_json::Value;

use crate::{
    services::error::ErrorService,
    tools::base::{Tool, ToolContext, ToolError},
};

static NEXT_LEASE: AtomicU64 = AtomicU64::new(1);

/// Owns every tool the agent can call.
///
/// Builtins and plugin tools share one map. A plugin's rows are tagged with its lease:
/// disabling the plugin drops them, and a publish that started before that drop is rejected.
/// A second counter on the same lease is the external process. Bumping it drops only that
/// plugin's per-user tools and rejects a publish from the process that just exited, while
/// the plugin's global tools stay until the plugin itself is disabled.
///
/// Tools are stored as `Arc<dyn Tool>` so a caller can hold one past an `.await` without
/// keeping this lock.
pub struct ToolService {
    tools: RwLock<HashMap<(Option<i64>, String), Registered>>,
}

struct Registered {
    tool: Arc<dyn Tool>,
    lease: Option<u64>,
}

/// Generations at the start of a publish. Compared again under the same lock that bumps them.
#[derive(Clone, Copy)]
struct ToolTicket {
    lease: u64,
    instance: u64,
    process: u64,
}

/// One plugin's handle on [`ToolService`]. Cloned into every `PluginEnv` for that plugin.
#[derive(Clone)]
pub struct ToolControl {
    service: Weak<ToolService>,
    lease: u64,
    instance: Arc<AtomicU64>,
    process: Arc<AtomicU64>,
}

impl ToolService {
    /// Panics if two static tools share a `function_name()`.
    pub fn new(tools: Vec<Box<dyn Tool>>) -> Self {
        let mut map = HashMap::with_capacity(tools.len());
        for tool in tools {
            let name = tool.function_name().to_string();
            let arc: Arc<dyn Tool> = Arc::from(tool);
            if map.insert((None, name.clone()), Registered { tool: arc, lease: None }).is_some() {
                panic!("Tool collision: multiple static tools registered with name '{name}'");
            }
        }
        Self { tools: RwLock::new(map) }
    }

    pub fn control(self: &Arc<Self>) -> ToolControl {
        ToolControl {
            service: Arc::downgrade(self),
            lease: NEXT_LEASE.fetch_add(1, Ordering::Relaxed),
            instance: Arc::new(AtomicU64::new(1)),
            process: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn get_tool(&self, user_id: i64, function_name: &str) -> Option<Arc<dyn Tool>> {
        let map = self.tools.read().await;
        map.get(&(Some(user_id), function_name.to_string()))
            .or_else(|| map.get(&(None, function_name.to_string())))
            .map(|entry| entry.tool.clone())
    }

    pub async fn call_tool(&self, function_name: &str, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let tool = self.get_tool(ctx.user_id, function_name).await.ok_or_else(|| {
            ToolError::FailedUnknown(format!("no tool named '{function_name}'"))
        })?;
        tool.call_untyped(data, ctx).await
    }

    pub async fn snapshot_tools(&self, user_id: i64) -> Vec<Arc<dyn Tool>> {
        let map = self.tools.read().await;
        let mut tools: Vec<_> = map.iter()
            .filter(|(key, _)| key.0.is_none() || key.0 == Some(user_id))
            .map(|(_, entry)| entry.tool.clone())
            .collect();
        tools.sort_by(|a, b| a.function_name().cmp(b.function_name()));
        tools
    }

    pub async fn user_deleted(&self, user_id: i64) {
        self.tools.write().await.retain(|key, _| key.0 != Some(user_id));
    }

    /// Global scope (`None`) meets every user. Two user scopes meet only when they are the same user.
    fn scopes_overlap(existing: Option<i64>, publishing: Option<i64>) -> bool {
        existing.is_none() || publishing.is_none() || existing == publishing
    }

    fn replace(
        map: &mut HashMap<(Option<i64>, String), Registered>,
        ticket: ToolTicket,
        instance: u64,
        process: u64,
        user_id: Option<i64>,
        tools: Vec<Box<dyn Tool>>,
    ) -> Result<(), ToolServiceError> {
        if ticket.instance != instance || ticket.process != process {
            return Err(ToolServiceError::Inactive);
        }
        let mut incoming = HashSet::new();
        for tool in &tools {
            let name = tool.function_name();
            if !incoming.insert(name) {
                return Err(ToolServiceError::Collision(name.to_string()));
            }
            // A global name is taken for every user. A per-user name is taken for that user
            // and also blocks a new global of the same name. This plugin's own rows in the
            // scope being replaced do not count: they are about to be removed.
            let taken = map.iter().any(|(key, entry)| {
                let same_scope = key.0 == user_id && entry.lease == Some(ticket.lease);
                key.1 == name && Self::scopes_overlap(key.0, user_id) && !same_scope
            });
            if taken {
                return Err(ToolServiceError::Collision(name.to_string()));
            }
        }
        map.retain(|key, entry| !(key.0 == user_id && entry.lease == Some(ticket.lease)));
        for tool in tools {
            map.insert((user_id, tool.function_name().to_string()), Registered {
                tool: Arc::from(tool),
                lease: Some(ticket.lease),
            });
        }
        Ok(())
    }
}

impl ToolControl {
    fn ticket(&self) -> ToolTicket {
        ToolTicket {
            lease: self.lease,
            instance: self.instance.load(Ordering::Relaxed),
            process: self.process.load(Ordering::Relaxed),
        }
    }

    /// Replaces this plugin's global tools. An empty list clears them. Per-user tools stay.
    pub async fn replace_global(&self, tools: Vec<Box<dyn Tool>>) -> Result<(), ToolServiceError> {
        self.publish(None, tools).await
    }

    /// Replaces this plugin's tools for one user. An empty list clears that user's list.
    pub async fn replace_user(&self, user_id: i64, tools: Vec<Box<dyn Tool>>) -> Result<(), ToolServiceError> {
        self.publish(Some(user_id), tools).await
    }

    async fn publish(&self, user_id: Option<i64>, tools: Vec<Box<dyn Tool>>) -> Result<(), ToolServiceError> {
        // Taken before the lock, so a disable or a dead process that lands while we wait cannot be written over.
        let ticket = self.ticket();
        let service = self.service.upgrade().ok_or(ToolServiceError::Inactive)?;
        let mut map = service.tools.write().await;
        // Bumps happen under this same lock, so a disable or a dead process cannot land in the middle of the check.
        ToolService::replace(
            &mut map,
            ticket,
            self.instance.load(Ordering::Relaxed),
            self.process.load(Ordering::Relaxed),
            user_id,
            tools,
        )
    }

    /// Plugin switched off or unloaded. Drops every tool it published.
    pub(crate) async fn end_instance(&self) {
        let Some(service) = self.service.upgrade() else { return };
        let mut map = service.tools.write().await;
        // Under the same lock as publish, so a ticket taken earlier fails the generation check.
        self.instance.fetch_add(1, Ordering::Relaxed);
        map.retain(|_, entry| entry.lease != Some(self.lease));
    }

    /// External process exited. Drops per-user tools and rejects that process's in-flight publish.
    /// Global tools stay; the next process republishes them on the same lease.
    pub(crate) async fn end_process(&self) {
        let Some(service) = self.service.upgrade() else { return };
        let mut map = service.tools.write().await;
        self.process.fetch_add(1, Ordering::Relaxed);
        map.retain(|key, entry| key.0.is_none() || entry.lease != Some(self.lease));
    }
}

#[derive(Debug)]
pub enum ToolServiceError {
    Collision(String),
    Inactive,
}

impl std::fmt::Display for ToolServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolServiceError::Inactive => write!(f, "the plugin is no longer enabled, or its process has exited"),
            ToolServiceError::Collision(name) => write!(f, "tool name '{name}' is already registered"),
        }
    }
}

impl From<ToolError> for ErrorService {
    fn from(err: ToolError) -> Self {
        ErrorService::internal(err.to_string())
    }
}
