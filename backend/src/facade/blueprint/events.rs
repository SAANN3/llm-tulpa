use super::runs::{NodeStatus, Progress, RunStatus, RunDetail};
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
    pub(super) fn run_changed(user_id: i64, run: &RunDetail, node_id: Option<&str>) -> Self {
        Self::RunChanged {
            user_id,
            blueprint_id: run.run.blueprint_id,
            run_id: run.run.id,
            status: run.run.status,
            current_node: run.state.current_node.clone(),
            node_id: node_id.map(str::to_owned),
            node_status: node_id
                .and_then(|id| run.state.nodes.get(id))
                .map(|n| n.status),
            progress: run.state.progress(),
        }
    }
    pub fn user_id(&self) -> i64 {
        match self {
            Self::QueueChanged { user_id } | Self::RunChanged { user_id, .. } => *user_id,
        }
    }
}
