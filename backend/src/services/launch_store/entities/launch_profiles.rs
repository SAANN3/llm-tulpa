use sea_orm::entity::prelude::*;

/// How a model is started — see `LaunchStore`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "launch_profiles")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub model_id: i64,
    pub name: String,
    pub mmproj_file: Option<String>,
    pub mmproj_gpu: bool,
    pub context_length: Option<i32>,
    pub cache_type_k: String,
    pub cache_type_v: String,
    pub flash_attn: bool,
    pub gpu_layers: i32,
    pub mtp: bool,
    pub spec_draft_n_max: i32,
    pub ngram_match: i32,
    pub ngram_min: i32,
    pub ngram_max: i32,
    pub extra_args: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
