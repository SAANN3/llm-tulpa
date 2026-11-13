pub mod catalog;
pub mod definition;
pub mod events;
mod execution;
mod runner;
pub mod runs;
mod template;
mod tool_nodes;
pub mod validation;

use crate::facade::agent::{Agent, Allowance};
use crate::services::{
    chat_store::ChatStore, tools::ToolService,
    blueprint_store::{BlueprintRecord, BlueprintStore, BlueprintStoreError},
    error::ErrorService,
    event_bus::EventBus,
};
use axum::http::StatusCode;
use catalog::{NodeDescriptor, NodeType};
use definition::{Schedule, Workflow};
use runner::Runner;
use tool_nodes::ToolNodes;
use runs::{QueueState, RunDetail, RunSummary};
use validation::{
    Validation,
    report::{Severity, ValidationReport},
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub struct BlueprintFacade {
    store: Arc<BlueprintStore>,
    validation: Validation,
    runner: Arc<Runner>,
}

impl BlueprintFacade {
    pub fn new(
        store: Arc<BlueprintStore>,
        validation: Validation,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
        agent: Arc<Agent>,
        chats: Arc<ChatStore>,
        tools: Arc<ToolService>,
    ) -> Self {
        let tool_nodes = ToolNodes::new(store.clone(), agent, chats, tools, events.clone());
        let runner = Runner::new(store.clone(), events, shutdown, tool_nodes);
        Self {
            store,
            validation,
            runner,
        }
    }

    pub async fn initialize(
        &self,
    ) -> Result<(), BlueprintStoreError> {
        self.runner.initialize().await
    }

    pub async fn start(
        &self,
        user_id: i64,
        id: i64,
        expected_revision: i64,
    ) -> Result<RunSummary, ErrorService> {
        let (record, workflow) = self.get(user_id, id).await?;
        if record.revision != expected_revision {
            return Err(BlueprintStoreError::Conflict.into());
        }
        let report = self.validation.check(user_id, &workflow).await?;
        if !report.valid {
            return Err(invalid(
                report
                    .diagnostics
                    .into_iter()
                    .find(|d| d.severity == Severity::Error)
                    .map(|d| d.message)
                    .unwrap_or_else(|| "invalid workflow".into()),
            ));
        }
        if let Some(node) = workflow
            .nodes
            .iter()
            .find(|n| !execution::supported(&n.kind))
        {
            return Err(invalid(format!(
                "node '{}': this node does not have an executor yet",
                node.id
            )));
        }
        let run = self
            .store
            .enqueue(user_id, id, expected_revision, &workflow)
            .await?;
        self.runner.queue_changed(user_id);
        self.runner.wake(user_id).await;
        Ok(run.run)
    }

    pub async fn queue(&self, user_id: i64) -> Result<QueueState, ErrorService> {
        Ok(self.store.queue(user_id).await?)
    }
    pub async fn reorder_queue(
        &self,
        user_id: i64,
        ids: &[i64],
    ) -> Result<QueueState, ErrorService> {
        self.store.reorder_queue(user_id, ids).await?;
        self.runner.queue_changed(user_id);
        Ok(self.store.queue(user_id).await?)
    }
    pub async fn clear_queue(&self, user_id: i64) -> Result<u64, ErrorService> {
        let count = self.store.clear_queue(user_id).await?;
        self.runner.queue_changed(user_id);
        Ok(count)
    }
    pub async fn stop(&self, user_id: i64) -> Result<Option<i64>, ErrorService> {
        Ok(self.runner.stop(user_id).await?)
    }
    pub async fn answer_tool(&self, user_id: i64, id: i64, run_id: i64, request_id: &str, allowance: Allowance) -> Result<(), ErrorService> {
        Ok(self.runner.answer_tool(user_id, id, run_id, request_id, allowance).await?)
    }
    pub async fn user_deleted(&self, user_id: i64) {
        self.runner.user_deleted(user_id).await;
    }
    pub async fn get_run(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
    ) -> Result<RunDetail, ErrorService> {
        Ok(self.store.get_run(user_id, id, run_id).await?)
    }
    pub async fn list_runs(
        &self,
        user_id: i64,
        id: i64,
        offset: u64,
        limit: u64,
    ) -> Result<(Vec<RunSummary>, u64), ErrorService> {
        if !(1..=50).contains(&limit) || offset > i64::MAX as u64 {
            return Err(invalid("limit must be 1..50 and offset must fit i64"));
        }
        Ok(self.store.list_runs(user_id, id, offset, limit).await?)
    }

    pub fn catalog(&self) -> Vec<NodeDescriptor> {
        NodeType::builtins()
    }

    pub async fn validate(
        &self,
        user_id: i64,
        workflow: &Workflow,
    ) -> Result<ValidationReport, ErrorService> {
        self.validation.check(user_id, workflow).await
    }

    pub async fn list(
        &self,
        user_id: i64,
        offset: u64,
        limit: u64,
        query: &str,
        updated: bool,
    ) -> Result<(Vec<BlueprintRecord>, u64), ErrorService> {
        if !(1..=100).contains(&limit) || offset > i64::MAX as u64 || query.chars().count() > 160 {
            return Err(invalid(
                "limit must be 1..100, offset must fit i64, and search must be at most 160 characters",
            ));
        }
        Ok(self
            .store
            .list(user_id, offset, limit, query, updated)
            .await?)
    }

    pub async fn get(
        &self,
        user_id: i64,
        id: i64,
    ) -> Result<(BlueprintRecord, Workflow), ErrorService> {
        let (record, bytes) = self.store.get(user_id, id).await?;
        let workflow = serde_json::from_slice(&bytes).map_err(|e| {
            tracing::error!(?e, blueprint_id = id, "could not decode stored workflow");
            ErrorService::internal("could not decode stored workflow")
        })?;
        validation::document(&workflow).map_err(|e| {
            tracing::error!(%e, blueprint_id = id, "invalid stored workflow");
            ErrorService::internal("invalid stored workflow")
        })?;
        Ok((record, workflow))
    }

    pub async fn create(
        &self,
        user_id: i64,
        name: &str,
        schedule: &Schedule,
        workflow: &Workflow,
    ) -> Result<BlueprintRecord, ErrorService> {
        validation::settings(name, schedule).map_err(invalid)?;
        let bytes = encode(workflow)?;
        Ok(self
            .store
            .create(user_id, name.trim(), schedule, &bytes)
            .await?)
    }

    pub async fn update(
        &self,
        user_id: i64,
        id: i64,
        expected_revision: i64,
        name: &str,
        schedule: &Schedule,
        workflow: Option<&Workflow>,
    ) -> Result<BlueprintRecord, ErrorService> {
        validation::settings(name, schedule).map_err(invalid)?;
        let bytes = workflow.map(encode).transpose()?;
        Ok(self
            .store
            .update(
                user_id,
                id,
                expected_revision,
                name.trim(),
                schedule,
                bytes.as_deref(),
            )
            .await?)
    }

    pub async fn delete(
        &self,
        user_id: i64,
        id: i64,
        expected_revision: i64,
    ) -> Result<(), ErrorService> {
        Ok(self.store.delete(user_id, id, expected_revision).await?)
    }
}

fn invalid(message: impl Into<String>) -> ErrorService {
    ErrorService::new(StatusCode::BAD_REQUEST, message)
}

fn encode(workflow: &Workflow) -> Result<Vec<u8>, ErrorService> {
    validation::document(workflow).map_err(|diagnostic| invalid(diagnostic.message))?;
    let bytes = serde_json::to_vec_pretty(workflow)
        .map_err(|e| invalid(format!("invalid workflow: {e}")))?;
    if bytes.len() > validation::MAX_DOCUMENT_BYTES {
        return Err(invalid("workflow exceeds 1 MiB"));
    }
    Ok(bytes)
}
