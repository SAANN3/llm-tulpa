use super::runs::{NodeStatus, Progress, RunStatus};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlueprintEvent {
    QueueChanged {
        user_id: i64,
    },
    RunChanged {
        user_id: i64,
        blueprint_id: i64,
        run_id: i64,
        status: RunStatus,
        current_node: Option<String>,
        node_id: Option<String>,
        node_status: Option<NodeStatus>,
        progress: Progress,
    },
}
impl BlueprintEvent {
    pub fn user_id(&self) -> i64 {
        match self {
            Self::QueueChanged { user_id } | Self::RunChanged { user_id, .. } => *user_id,
        }
    }
}
