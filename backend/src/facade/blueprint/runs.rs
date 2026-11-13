use super::definition::{Literal, Workflow};
use crate::facade::agent::{AgentToolCall, Allowance};
use chrono::{DateTime, Utc};
use sea_orm::{DeriveActiveEnum, EnumIter, sea_query::StringLen};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    ToSchema,
    EnumIter,
    DeriveActiveEnum,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    #[sea_orm(string_value = "queued")]
    Queued,
    #[sea_orm(string_value = "running")]
    Running,
    #[sea_orm(string_value = "waiting_for_permission")]
    WaitingForPermission,
    #[sea_orm(string_value = "stopping")]
    Stopping,
    #[sea_orm(string_value = "completed")]
    Completed,
    #[sea_orm(string_value = "failed")]
    Failed,
    #[sea_orm(string_value = "cancelled")]
    Cancelled,
    #[sea_orm(string_value = "interrupted")]
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Pending,
    WaitingForPermission,
    Running,
    Completed,
    Failed,
    Skipped,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NodeProgress {
    #[serde(default)]
    pub chat_id: Option<i64>,
    pub status: NodeStatus,
    pub outputs: BTreeMap<String, Literal>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct PendingTool {
    pub request_id: String,
    pub node_id: String,
    pub call: AgentToolCall,
    pub answer: Option<Allowance>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RunState {
    #[serde(default)]
    pub pending_tool: Option<PendingTool>,
    pub nodes: BTreeMap<String, NodeProgress>,
    pub current_node: Option<String>,
    pub error: Option<String>,
}
impl RunState {
    pub fn new(workflow: &Workflow) -> Self {
        Self {
            pending_tool: None,
            nodes: workflow
                .nodes
                .iter()
                .map(|n| {
                    (
                        n.id.clone(),
                        NodeProgress {
                            chat_id: None,
                            status: NodeStatus::Pending,
                            outputs: BTreeMap::new(),
                            error: None,
                        },
                    )
                })
                .collect(),
            current_node: None,
            error: None,
        }
    }
    pub fn progress(&self) -> Progress {
        let completed = self
            .nodes
            .values()
            .filter(|n| n.status == NodeStatus::Completed)
            .count() as u32;
        let skipped = self
            .nodes
            .values()
            .filter(|n| n.status == NodeStatus::Skipped)
            .count() as u32;
        let total = self.nodes.len() as u32 - skipped;
        Progress {
            completed,
            skipped,
            total: Some(total),
            percent: Some(if total == 0 {
                100.0
            } else {
                completed as f64 * 100.0 / total as f64
            }),
        }
    }
    pub fn terminate(&mut self, status: NodeStatus) {
        for node in self.nodes.values_mut() {
            if matches!(node.status, NodeStatus::Pending | NodeStatus::Running | NodeStatus::WaitingForPermission) {
                node.status = status;
            }
        }
        self.current_node = None;
        self.pending_tool = None;
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Progress {
    pub completed: u32,
    pub skipped: u32,
    pub total: Option<u32>,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RunSummary {
    pub id: i64,
    pub blueprint_id: i64,
    pub revision: i64,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub status: RunStatus,
    pub queue_position: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub current_node: Option<String>,
    pub progress: Progress,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RunDetail {
    pub chat_id: Option<i64>,
    pub run: RunSummary,
    pub workflow: Workflow,
    pub state: RunState,
}

#[derive(Serialize, ToSchema)]
pub struct QueueState {
    pub running: Option<RunSummary>,
    pub queued: Vec<RunSummary>,
}
