use std::collections::BTreeMap;

use sea_orm::{ActiveValue::Set, ConnectionTrait, QuerySelect, TransactionTrait, sea_query::OnConflict, prelude::*};

use super::{BlueprintStore, BlueprintStoreError, entities::{blueprint_runs, blueprint_variables as entity}};
use crate::facade::blueprint::{definition::Literal, runs::RunStatus};

const MAX_VARIABLES_BYTES: usize = 4 * 1024 * 1024;

impl BlueprintStore {
    pub(super) async fn saved_variables<C: ConnectionTrait>(
        db: &C, id: i64,
    ) -> Result<BTreeMap<String, Literal>, BlueprintStoreError> {
        match entity::Entity::find_by_id(id).one(db).await? {
            Some(row) => Ok(serde_json::from_value(row.values)?),
            None => Ok(BTreeMap::new()),
        }
    }

    // The caller holds the blueprint row lock, including before the first values row exists.
    pub(super) async fn write_variables<C: ConnectionTrait>(
        db: &C, id: i64, saved: &BTreeMap<String, Literal>,
    ) -> Result<(), BlueprintStoreError> {
        if serde_json::to_vec(saved)?.len() > MAX_VARIABLES_BYTES {
            return Err(BlueprintStoreError::VariableState("persistent variables exceed 4 MiB; reset unused saved values".into()));
        }
        entity::Entity::insert(entity::ActiveModel {
            blueprint_id: Set(id), values: Set(serde_json::to_value(saved)?),
        }).on_conflict(OnConflict::column(entity::Column::BlueprintId)
            .update_column(entity::Column::Values).to_owned()).exec(db).await?;
        Ok(())
    }

    pub async fn variables(&self, user_id: i64, id: i64) -> Result<BTreeMap<String, Literal>, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        Self::owned(user_id, id).lock_shared().one(&txn).await?.ok_or(BlueprintStoreError::NotFound)?;
        let saved = Self::saved_variables(&txn, id).await?;
        txn.commit().await?;
        Ok(saved)
    }

    pub async fn reset_variables(
        &self, user_id: i64, id: i64, expected_revision: i64, node_id: Option<&str>,
    ) -> Result<(), BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        let row = self.lock_blueprint(&txn, user_id, id).await?;
        if row.revision != expected_revision { return Err(BlueprintStoreError::Conflict); }
        if blueprint_runs::Entity::find()
            .filter(blueprint_runs::Column::BlueprintId.eq(id))
            .filter(blueprint_runs::Column::Status.is_in([RunStatus::Running, RunStatus::WaitingForPermission, RunStatus::Stopping]))
            .one(&txn).await?.is_some()
        {
            return Err(BlueprintStoreError::VariablesBusy);
        }
        let mut saved = Self::saved_variables(&txn, id).await?;
        match node_id {
            Some(node_id) => { saved.remove(node_id); }
            None => saved.clear(),
        }
        Self::write_variables(&txn, id, &saved).await?;
        txn.commit().await?;
        Ok(())
    }
}
