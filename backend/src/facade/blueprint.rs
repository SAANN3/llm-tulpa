pub mod definition;
pub mod validation;

use crate::services::{
    blueprint_store::{BlueprintRecord, BlueprintStore},
    error::ErrorService,
};
use axum::http::StatusCode;
use definition::{Schedule, Workflow};
use std::sync::Arc;

pub struct BlueprintFacade {
    store: Arc<BlueprintStore>,
}

impl BlueprintFacade {
    pub fn new(store: Arc<BlueprintStore>) -> Self {
        Self { store }
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
    validation::document(workflow).map_err(invalid)?;
    let bytes = serde_json::to_vec_pretty(workflow)
        .map_err(|e| invalid(format!("invalid workflow: {e}")))?;
    if bytes.len() > validation::MAX_DOCUMENT_BYTES {
        return Err(invalid("workflow exceeds 1 MiB"));
    }
    Ok(bytes)
}
