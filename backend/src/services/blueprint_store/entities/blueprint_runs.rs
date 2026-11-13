use crate::facade::blueprint::runs::RunStatus;
use sea_orm::entity::prelude::*;

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
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
