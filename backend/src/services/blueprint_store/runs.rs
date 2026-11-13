use super::{
    BlueprintStore, BlueprintStoreError, ScheduledTick,
    entities::{blueprint_runs as runs, blueprints, users},
};
use crate::facade::blueprint::{
    definition::Workflow,
    runs::{NodeStatus, QueueState, RunDetail, RunState, RunStatus, RunSummary},
};
use chrono::Utc;
use crate::facade::agent::{Allowance, Decision};
use sea_orm::{
    ActiveValue::Set, DatabaseTransaction, QueryOrder, QuerySelect, Select, TransactionTrait, prelude::*,
};
use std::collections::HashSet;

fn summary(row: &runs::Model, state: &RunState) -> RunSummary {
    RunSummary {
        id: row.id,
        blueprint_id: row.blueprint_id,
        revision: row.revision,
        scheduled_for: row.scheduled_for,
        status: row.status,
        queue_position: row.queue_position,
        created_at: row.created_at,
        started_at: row.started_at,
        finished_at: row.finished_at,
        current_node: state.current_node.clone(),
        progress: state.progress(),
        error: state.error.clone(),
    }
}
fn detail(row: runs::Model) -> Result<RunDetail, BlueprintStoreError> {
    let state = serde_json::from_value::<RunState>(row.state.clone())?;
    Ok(RunDetail {
        chat_id: row.chat_id,
        run: summary(&row, &state),
        workflow: serde_json::from_value(row.workflow)?,
        state,
    })
}
fn run_query(blueprint_id: i64, id: i64) -> Select<runs::Entity> {
    runs::Entity::find_by_id(id).filter(runs::Column::BlueprintId.eq(blueprint_id))
}

impl BlueprintStore {
    async fn lock_queue(
        &self,
        txn: &DatabaseTransaction,
        user_id: i64,
    ) -> Result<(), BlueprintStoreError> {
        users::Entity::find_by_id(user_id)
            .lock_exclusive()
            .one(txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        Ok(())
    }

    pub(super) async fn lock_blueprint(
        &self,
        txn: &DatabaseTransaction,
        user_id: i64,
        id: i64,
    ) -> Result<blueprints::Model, BlueprintStoreError> {
        Ok(Self::owned(user_id, id)
            .lock_exclusive()
            .one(txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?)
    }

    pub async fn enqueue(
        &self,
        user_id: i64,
        id: i64,
        revision: i64,
        workflow: &Workflow,
        scheduled: Option<&ScheduledTick>,
        error: Option<String>,
    ) -> Result<RunDetail, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        let blueprint = self.lock_blueprint(&txn, user_id, id).await?;
        if blueprint.revision != revision {
            return Err(BlueprintStoreError::Conflict);
        }
        if let Some(tick) = scheduled {
            if blueprint.next_due_at != Some(tick.due_at) {
                return Err(BlueprintStoreError::Conflict);
            }
            blueprints::ActiveModel {
                id: Set(id), next_due_at: Set(Some(tick.next_due_at)), ..Default::default()
            }.update(&txn).await?;
        }
        let mut state = RunState::new(workflow);
        let failed = error.is_some();
        if failed { state.terminate(NodeStatus::Skipped); }
        state.error = error;
        let position = if failed {
            None
        } else {
            let last = runs::Entity::find()
                .filter(runs::Column::UserId.eq(user_id))
                .filter(runs::Column::Status.eq(RunStatus::Queued))
                .order_by_desc(runs::Column::QueuePosition)
                .one(&txn).await?;
            Some(last.and_then(|r| r.queue_position).unwrap_or(0).checked_add(1)
                .ok_or_else(|| BlueprintStoreError::Corrupt("queue position overflow".into()))?)
        };
        let row = runs::ActiveModel {
            blueprint_id: Set(id),
            user_id: Set(user_id),
            revision: Set(revision),
            scheduled_for: Set(scheduled.map(|t| t.due_at)),
            workflow: Set(serde_json::to_value(workflow)?),
            state: Set(serde_json::to_value(state)?),
            status: Set(if failed { RunStatus::Failed } else { RunStatus::Queued }),
            queue_position: Set(position),
            finished_at: Set(failed.then(Utc::now)),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        txn.commit().await?;
        detail(row)
    }

    pub async fn queue(&self, user_id: i64) -> Result<QueueState, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        let rows = runs::Entity::find()
            .filter(runs::Column::UserId.eq(user_id))
            .filter(runs::Column::Status.is_in([
                RunStatus::Queued,
                RunStatus::Running,
                RunStatus::WaitingForPermission,
                RunStatus::Stopping,
            ]))
            .order_by_asc(runs::Column::QueuePosition)
            .all(&txn)
            .await?;
        let mut result = QueueState {
            running: None,
            queued: vec![],
        };
        for row in rows {
            let state = serde_json::from_value(row.state.clone())?;
            let out = summary(&row, &state);
            if row.status == RunStatus::Queued {
                result.queued.push(out);
            } else {
                result.running = Some(out);
            }
        }
        txn.commit().await?;
        Ok(result)
    }

    pub async fn reorder_queue(
        &self,
        user_id: i64,
        ids: &[i64],
    ) -> Result<(), BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        let rows = runs::Entity::find()
            .filter(runs::Column::UserId.eq(user_id))
            .filter(runs::Column::Status.eq(RunStatus::Queued))
            .all(&txn)
            .await?;
        let requested: HashSet<_> = ids.iter().copied().collect();
        if requested.len() != ids.len() || requested != rows.iter().map(|r| r.id).collect() {
            return Err(BlueprintStoreError::QueueChanged);
        }
        // The deferred uniqueness constraint permits swaps inside this transaction only.
        for (position, run_id) in ids.iter().enumerate() {
            runs::ActiveModel {
                id: Set(*run_id),
                queue_position: Set(Some(position as i64 + 1)),
                ..Default::default()
            }
            .update(&txn)
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn clear_queue(&self, user_id: i64) -> Result<u64, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        let rows = runs::Entity::find()
            .filter(runs::Column::UserId.eq(user_id))
            .filter(runs::Column::Status.eq(RunStatus::Queued))
            .all(&txn)
            .await?;
        let count = rows.len() as u64;
        for row in rows {
            let mut state: RunState = serde_json::from_value(row.state.clone())?;
            state.terminate(NodeStatus::Cancelled);
            let mut row: runs::ActiveModel = row.into();
            row.status = Set(RunStatus::Cancelled);
            row.queue_position = Set(None);
            row.state = Set(serde_json::to_value(state)?);
            row.finished_at = Set(Some(Utc::now()));
            row.update(&txn).await?;
        }
        txn.commit().await?;
        Ok(count)
    }

    pub async fn get_run(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
    ) -> Result<RunDetail, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let row = run_query(id, run_id)
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        txn.commit().await?;
        detail(row)
    }

    pub async fn list_runs(
        &self,
        user_id: i64,
        id: i64,
        offset: u64,
        limit: u64,
    ) -> Result<(Vec<RunSummary>, u64), BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let query = runs::Entity::find().filter(runs::Column::BlueprintId.eq(id));
        let count = query.clone().count(&txn).await?;
        let rows = query
            .order_by_desc(runs::Column::Id)
            .offset(offset)
            .limit(limit)
            .all(&txn)
            .await?;
        let mut result = Vec::new();
        for row in rows {
            let state = serde_json::from_value(row.state.clone())?;
            result.push(summary(&row, &state));
        }
        txn.commit().await?;
        Ok((result, count))
    }

    pub async fn claim_run(&self, user_id: i64) -> Result<Option<RunDetail>, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        if runs::Entity::find()
            .filter(runs::Column::UserId.eq(user_id))
            .filter(runs::Column::Status.is_in([RunStatus::Running, RunStatus::WaitingForPermission, RunStatus::Stopping]))
            .one(&txn)
            .await?
            .is_some()
        {
            txn.commit().await?;
            return Ok(None);
        }
        let row = runs::Entity::find()
            .filter(runs::Column::UserId.eq(user_id))
            .filter(runs::Column::Status.eq(RunStatus::Queued))
            .order_by_asc(runs::Column::QueuePosition)
            .one(&txn)
            .await?;
        let result = if let Some(row) = row {
            let mut row: runs::ActiveModel = row.into();
            row.status = Set(RunStatus::Running);
            row.queue_position = Set(None);
            row.started_at = Set(Some(Utc::now()));
            Some(detail(row.update(&txn).await?)?)
        } else {
            None
        };
        txn.commit().await?;
        Ok(result)
    }

    pub async fn request_stop(
        &self,
        user_id: i64,
    ) -> Result<Option<RunDetail>, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        let row = runs::Entity::find()
            .filter(runs::Column::UserId.eq(user_id))
            .filter(runs::Column::Status.is_in([RunStatus::Running, RunStatus::WaitingForPermission, RunStatus::Stopping]))
            .one(&txn)
            .await?;
        let result = if let Some(row) = row {
            let mut row: runs::ActiveModel = row.into();
            row.status = Set(RunStatus::Stopping);
            Some(detail(row.update(&txn).await?)?)
        } else {
            None
        };
        txn.commit().await?;
        Ok(result)
    }

    /// False means a stop won the race; the worker must not begin another node.
    pub async fn save_progress(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
        state: &RunState,
    ) -> Result<bool, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let row = run_query(id, run_id)
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        if row.status != RunStatus::Running {
            txn.commit().await?;
            return Ok(false);
        }
        let mut row: runs::ActiveModel = row.into();
        row.state = Set(serde_json::to_value(state)?);
        row.update(&txn).await?;
        txn.commit().await?;
        Ok(true)
    }

    pub async fn finish_run(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
        status: RunStatus,
        mut state: RunState,
    ) -> Result<RunDetail, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let row = run_query(id, run_id)
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        if !matches!(row.status, RunStatus::Running | RunStatus::WaitingForPermission | RunStatus::Stopping) {
            return Err(BlueprintStoreError::Conflict);
        }
        let status = if row.status == RunStatus::Stopping {
            RunStatus::Cancelled
        } else {
            status
        };
        let end = match status {
            RunStatus::Completed => NodeStatus::Skipped,
            RunStatus::Interrupted => NodeStatus::Interrupted,
            _ => NodeStatus::Cancelled,
        };
        state.terminate(end);
        let mut row: runs::ActiveModel = row.into();
        row.status = Set(status);
        row.state = Set(serde_json::to_value(state)?);
        row.finished_at = Set(Some(Utc::now()));
        let result = detail(row.update(&txn).await?)?;
        txn.commit().await?;
        Ok(result)
    }

    /// One backend owns the runtime. Startup never replays a run that may have performed an action.
    pub async fn recover_runs(&self) -> Result<Vec<i64>, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        for row in runs::Entity::find()
            .filter(runs::Column::Status.is_in([RunStatus::Running, RunStatus::WaitingForPermission, RunStatus::Stopping]))
            .all(&txn)
            .await?
        {
            let mut state: RunState = serde_json::from_value(row.state.clone())?;
            state.terminate(NodeStatus::Interrupted);
            state.error = Some("backend stopped before the run finished".into());
            let mut row: runs::ActiveModel = row.into();
            row.status = Set(RunStatus::Interrupted);
            row.state = Set(serde_json::to_value(state)?);
            row.finished_at = Set(Some(Utc::now()));
            row.update(&txn).await?;
        }
        let owners: HashSet<_> = runs::Entity::find()
            .filter(runs::Column::Status.eq(RunStatus::Queued))
            .all(&txn)
            .await?
            .into_iter()
            .map(|r| r.user_id)
            .collect();
        txn.commit().await?;
        Ok(owners.into_iter().collect())
    }
}

impl BlueprintStore {
    pub async fn attach_run_chat(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
        chat_id: i64,
    ) -> Result<bool, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let row = run_query(id, run_id)
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        if row.status != RunStatus::Running {
            return Ok(false);
        }
        let mut row: runs::ActiveModel = row.into();
        row.chat_id = Set(Some(chat_id));
        row.update(&txn).await?;
        txn.commit().await?;
        Ok(true)
    }

    pub async fn wait_for_permission(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
        state: &RunState,
    ) -> Result<bool, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let row = run_query(id, run_id)
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        if row.status != RunStatus::Running {
            return Ok(false);
        }
        let mut row: runs::ActiveModel = row.into();
        row.status = Set(RunStatus::WaitingForPermission);
        row.state = Set(serde_json::to_value(state)?);
        row.update(&txn).await?;
        txn.commit().await?;
        Ok(true)
    }

    pub async fn answer_tool(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
        request_id: &str,
        allowance: Allowance,
    ) -> Result<(), BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let row = run_query(id, run_id)
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        let mut state: RunState = serde_json::from_value(row.state.clone())?;
        let pending = state
            .pending_tool
            .as_mut()
            .filter(|p| p.request_id == request_id && p.answer.is_none())
            .ok_or(BlueprintStoreError::ConfirmationChanged)?;
        if row.status != RunStatus::WaitingForPermission {
            return Err(BlueprintStoreError::ConfirmationChanged);
        }
        pending
            .call
            .check_decision(&Decision {
                index: 0,
                allowance,
            })
            .map_err(BlueprintStoreError::Corrupt)?;
        pending.answer = Some(allowance);
        let mut row: runs::ActiveModel = row.into();
        row.state = Set(serde_json::to_value(state)?);
        row.update(&txn).await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn resume_tool(
        &self,
        user_id: i64,
        id: i64,
        run_id: i64,
        state: &RunState,
    ) -> Result<bool, BlueprintStoreError> {
        let txn = self.db.begin().await?;
        self.lock_queue(&txn, user_id).await?;
        self.lock_blueprint(&txn, user_id, id).await?;
        let row = run_query(id, run_id)
            .one(&txn)
            .await?
            .ok_or(BlueprintStoreError::NotFound)?;
        if row.status != RunStatus::WaitingForPermission {
            return Ok(false);
        }
        let mut row: runs::ActiveModel = row.into();
        row.status = Set(RunStatus::Running);
        row.state = Set(serde_json::to_value(state)?);
        row.update(&txn).await?;
        txn.commit().await?;
        Ok(true)
    }
}
