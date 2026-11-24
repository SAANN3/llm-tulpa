use super::{BlueprintRecord, BlueprintStore, BlueprintStoreError, entities::blueprints};
use crate::facade::blueprint::definition::Schedule;
use chrono::{DateTime, Utc};
use sea_orm::{ActiveValue::Set, QueryOrder, prelude::*};

pub struct ScheduledTick {
    pub due_at: DateTime<Utc>,
    pub next_due_at: DateTime<Utc>,
}

impl BlueprintStore {
    pub async fn next_scheduled(&self) -> Result<Option<BlueprintRecord>, BlueprintStoreError> {
        blueprints::Entity::find()
            .filter(blueprints::Column::NextDueAt.is_not_null())
            .order_by_asc(blueprints::Column::NextDueAt)
            .order_by_asc(blueprints::Column::Id)
            .one(&self.db)
            .await?
            .map(TryInto::try_into)
            .transpose()
    }

    pub async fn initialize_schedules(&self) -> Result<(), BlueprintStoreError> {
        // Existing settings predate scheduling. Anchor them at their saved time so downtime is caught up once.
        let rows = blueprints::Entity::find()
            .filter(blueprints::Column::NextDueAt.is_null())
            .filter(blueprints::Column::Schedule.ne(serde_json::to_value(Schedule::Manual)?))
            .all(&self.db)
            .await?;
        for row in rows {
            let schedule: Schedule = serde_json::from_value(row.schedule)?;
            let next = schedule
                .next_after(row.updated_at)
                .map_err(BlueprintStoreError::Corrupt)?;
            blueprints::Entity::update_many()
                .set(blueprints::ActiveModel {
                    next_due_at: Set(next),
                    ..Default::default()
                })
                .filter(blueprints::Column::Id.eq(row.id))
                .filter(blueprints::Column::Revision.eq(row.revision))
                .filter(blueprints::Column::NextDueAt.is_null())
                .exec(&self.db)
                .await?;
        }
        Ok(())
    }
}
