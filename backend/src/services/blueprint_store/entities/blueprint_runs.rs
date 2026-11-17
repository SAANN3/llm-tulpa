use crate::{
    facade::blueprint::runs::{NodeStatus, RunState, RunStatus},
    services::blueprint_store::BlueprintStoreError,
};
use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseTransaction, entity::prelude::*};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "blueprint_runs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub blueprint_id: i64,
    pub user_id: i64,
    pub chat_id: Option<i64>,
    pub revision: i64,
    pub scheduled_for: Option<DateTimeUtc>,
    pub workflow: Json,
    pub state: Json,
    pub status: RunStatus,
    pub queue_position: Option<i64>,
    pub created_at: DateTimeUtc,
    pub started_at: Option<DateTimeUtc>,
    pub finished_at: Option<DateTimeUtc>,
}

impl Model {
    pub async fn cancel_queued(self, txn: &DatabaseTransaction) -> Result<(), BlueprintStoreError> {
        if self.status != RunStatus::Queued {
            return Err(BlueprintStoreError::NotQueued);
        }
        let mut state: RunState = serde_json::from_value(self.state.clone())?;
        state.terminate(NodeStatus::Cancelled);
        let mut row: ActiveModel = self.into();
        row.status = Set(RunStatus::Cancelled);
        row.queue_position = Set(None);
        row.state = Set(serde_json::to_value(state)?);
        row.finished_at = Set(Some(Utc::now()));
        row.update(txn).await?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
