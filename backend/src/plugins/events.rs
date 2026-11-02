//! Awaited plugin observers. A subscription records the plugin's current generation, the same
//! pair as its tools: disabling the plugin or exiting its process makes that subscription stop
//! matching, and a later subscribe records the new generation.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock as AsyncRwLock;

use super::base::{Plugin, PluginError};
use crate::facade::agent::compaction::CompactionStatus;
use crate::facade::agent::{RunEnded, TurnState};
use crate::services::chat_store::{Chat, Message, ToolCallOut};
use crate::services::job_store::JobRecord;

const OBSERVER_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginEventKind {
    MessageStored,
    RunStarted,
    RunFinished,
    ToolStarted,
    ToolFinished,
    ChatCreated,
    ChatDeleted,
    MessagesRemoved,
    ModelChanged,
    JobStarted,
    JobFinished,
    CompactionStarted,
    CompactionFinished,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginEvent<'a> {
    MessageStored { user_id: i64, message: &'a Message },
    RunStarted { user_id: i64, chat_id: i64, parent_chat_id: Option<i64>, state: &'a TurnState },
    RunFinished { user_id: i64, chat_id: i64, state: &'a RunEnded },
    ToolStarted { user_id: i64, chat_id: i64, call: &'a ToolCallOut },
    ToolFinished { user_id: i64, chat_id: i64, call: &'a ToolCallOut, result: &'a Message },
    ChatCreated { user_id: i64, chat_id: i64, state: &'a Chat },
    ChatDeleted { user_id: i64, chat_id: i64 },
    MessagesRemoved { user_id: i64, chat_id: i64, message_ids: &'a [i64] },
    ModelChanged { user_id: i64, chat_id: i64, model_id: i64, launch_profile_id: Option<i64> },
    JobStarted { user_id: i64, chat_id: i64, state: &'a JobRecord },
    JobFinished { user_id: i64, chat_id: i64, state: &'a JobRecord },
    CompactionStarted { user_id: i64, chat_id: i64, status: CompactionStatus, state: &'a Chat, started_at: DateTime<Utc>, prompt_tokens: Option<u64>, context_tokens: u64 },
    CompactionFinished { user_id: i64, chat_id: i64, status: CompactionStatus, state: &'a Chat, started_at: DateTime<Utc>, detail: Option<&'a str> },
}

impl From<&PluginEvent<'_>> for PluginEventKind {
    fn from(event: &PluginEvent<'_>) -> Self {
        match event {
            PluginEvent::MessageStored { .. } => Self::MessageStored,
            PluginEvent::RunStarted { .. } => Self::RunStarted,
            PluginEvent::RunFinished { .. } => Self::RunFinished,
            PluginEvent::ToolStarted { .. } => Self::ToolStarted,
            PluginEvent::ToolFinished { .. } => Self::ToolFinished,
            PluginEvent::ChatCreated { .. } => Self::ChatCreated,
            PluginEvent::ChatDeleted { .. } => Self::ChatDeleted,
            PluginEvent::MessagesRemoved { .. } => Self::MessagesRemoved,
            PluginEvent::ModelChanged { .. } => Self::ModelChanged,
            PluginEvent::JobStarted { .. } => Self::JobStarted,
            PluginEvent::JobFinished { .. } => Self::JobFinished,
            PluginEvent::CompactionStarted { .. } => Self::CompactionStarted,
            PluginEvent::CompactionFinished { .. } => Self::CompactionFinished,
        }
    }
}

struct Subscription {
    plugin: Weak<dyn Plugin>,
    instance: Arc<AtomicU64>,
    process: Arc<AtomicU64>,
    seen_instance: u64,
    seen_process: u64,
}

impl Subscription {
    fn alive(&self) -> bool {
        self.instance.load(Ordering::Relaxed) == self.seen_instance
            && self.process.load(Ordering::Relaxed) == self.seen_process
    }
}

#[derive(Default)]
pub struct PluginEvents {
    by_kind: AsyncRwLock<HashMap<PluginEventKind, Vec<Subscription>>>,
}

impl PluginEvents {
    pub fn control(self: &Arc<Self>) -> EventControl {
        EventControl {
            bus: Arc::downgrade(self),
            plugin: Arc::new(RwLock::new(None)),
            instance: Arc::new(AtomicU64::new(1)),
            process: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn has_subscribers(&self, kind: PluginEventKind) -> bool {
        self.by_kind.read().await.get(&kind).is_some_and(|list| list.iter().any(Subscription::alive))
    }

    /// Waits for matching observers. A slow or panicking plugin is logged and does not fail the caller.
    pub async fn fire(&self, event: PluginEvent<'_>) {
        let kind = PluginEventKind::from(&event);
        let plugins = {
            let mut by_kind = self.by_kind.write().await;
            let Some(list) = by_kind.get_mut(&kind) else { return };
            list.retain(Subscription::alive);
            list.iter().filter_map(|sub| sub.plugin.upgrade()).collect::<Vec<_>>()
        };
        for plugin in plugins {
            let label = format!("{}/{}", plugin.plugin_name(), plugin.plugin_subname());
            let callback = AssertUnwindSafe(plugin.on_event(&event)).catch_unwind();
            match tokio::time::timeout(OBSERVER_TIMEOUT, callback).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(e))) => tracing::warn!("plugin {label} observer failed: {e}"),
                Ok(Err(_)) => tracing::warn!("plugin {label} observer panicked"),
                Err(_) => tracing::warn!("plugin {label} observer timed out"),
            }
        }
    }
}

/// One plugin's subscribe/unsubscribe handle. Created with its [`PluginEnv`].
#[derive(Clone)]
pub struct EventControl {
    #[allow(dead_code)] // read by subscribe, which plugins call once they observe events
    bus: Weak<PluginEvents>,
    plugin: Arc<RwLock<Option<Weak<dyn Plugin>>>>,
    instance: Arc<AtomicU64>,
    process: Arc<AtomicU64>,
}

#[allow(dead_code)]
impl EventControl {
    pub(crate) fn attach(&self, plugin: &Arc<dyn Plugin>) {
        *self.plugin.write().unwrap() = Some(Arc::downgrade(plugin));
    }

    pub async fn subscribe(&self, kind: PluginEventKind) -> Result<(), PluginError> {
        self.set(kind, true).await
    }

    pub async fn unsubscribe(&self, kind: PluginEventKind) -> Result<(), PluginError> {
        self.set(kind, false).await
    }

    async fn set(&self, kind: PluginEventKind, on: bool) -> Result<(), PluginError> {
        let bus = self.bus.upgrade().ok_or_else(inactive)?;
        let plugin = self.plugin.read().unwrap().clone().ok_or_else(inactive)?;
        if plugin.strong_count() == 0 {
            return Err(inactive());
        }
        let seen_instance = self.instance.load(Ordering::Relaxed);
        let seen_process = self.process.load(Ordering::Relaxed);
        let mut by_kind = bus.by_kind.write().await;
        let list = by_kind.entry(kind).or_default();
        list.retain(|sub| !(Arc::ptr_eq(&sub.instance, &self.instance) && sub.alive()));
        if on {
            list.push(Subscription {
                plugin,
                instance: self.instance.clone(),
                process: self.process.clone(),
                seen_instance,
                seen_process,
            });
        }
        Ok(())
    }

    pub(crate) async fn end_instance(&self) {
        self.instance.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) async fn end_process(&self) {
        self.process.fetch_add(1, Ordering::Relaxed);
    }
}

#[allow(dead_code)]
fn inactive() -> PluginError {
    PluginError::InvalidState("plugin event subscription is no longer active".to_string())
}
