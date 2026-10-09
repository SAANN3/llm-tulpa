mod entities;

use axum::http::StatusCode;
use entities::{launch_profiles, model_loads};
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, DbErr, QueryOrder, QuerySelect, SqlErr};

use crate::services::error::ErrorService;

/// Owns the `launch_profiles` table: how a model is started (which projector, how much context,
/// what the KV cache is stored as, whether it drafts tokens with MTP). Global rather than per user,
/// because it is the hardware's business: the owner writes profiles, everyone reads and chooses
/// among them. A model can have several, e.g. a long-context one and a vision one. Also owns
/// `model_loads`: every start of the model server under a profile, and how long it took.
pub struct LaunchStore {
    db: DatabaseConnection,
}

/// KV cache storage types llama-server accepts for `--cache-type-k`/`-v`.
pub const CACHE_TYPES: [&str; 9] = ["f32", "f16", "bf16", "q8_0", "q4_0", "q4_1", "iq4_nl", "q5_0", "q5_1"];

const NAME_UNIQUE: &str = "launch_profiles_name_unique";

#[derive(Clone, Debug)]
pub struct LaunchProfile {
    pub id: i64,
    pub model_id: i64,
    pub name: String,
    /// A projector file in the model folder, giving the model vision
    pub mmproj_file: Option<String>,
    /// Whether the projector runs on the GPU (the default) or the CPU, which saves its VRAM at the
    /// cost of slower image reading
    pub mmproj_gpu: bool,
    /// `None` sizes the context to free memory (llama-server's `--fit`)
    pub context_length: Option<i32>,
    pub cache_type_k: String,
    pub cache_type_v: String,
    pub flash_attn: bool,
    /// How many layers go to the GPU (`-ngl`); 99 means all of them
    pub gpu_layers: i32,
    /// Draft tokens with the model's own MTP head, when the file has one
    pub mtp: bool,
    pub spec_draft_n_max: i32,
    pub ngram_match: i32,
    pub ngram_min: i32,
    pub ngram_max: i32,
    /// Extra command-line arguments, whitespace-separated. Never passed through a shell.
    pub extra_args: String,
}

impl From<launch_profiles::Model> for LaunchProfile {
    fn from(m: launch_profiles::Model) -> Self {
        Self {
            id: m.id,
            model_id: m.model_id,
            name: m.name,
            mmproj_file: m.mmproj_file,
            mmproj_gpu: m.mmproj_gpu,
            context_length: m.context_length,
            cache_type_k: m.cache_type_k,
            cache_type_v: m.cache_type_v,
            flash_attn: m.flash_attn,
            gpu_layers: m.gpu_layers,
            mtp: m.mtp,
            spec_draft_n_max: m.spec_draft_n_max,
            ngram_match: m.ngram_match,
            ngram_min: m.ngram_min,
            ngram_max: m.ngram_max,
            extra_args: m.extra_args,
        }
    }
}

/// What a request may set on a profile: everything but its ids and timestamps.
#[derive(Clone, Debug)]
pub struct LaunchInput {
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
}

impl Default for LaunchInput {
    /// The defaults a new profile starts from: the whole model on the GPU, flash attention on, an
    /// 8-bit KV cache, and MTP drafting when the file supports it.
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            mmproj_file: None,
            mmproj_gpu: true,
            context_length: None,
            cache_type_k: "q8_0".to_string(),
            cache_type_v: "q8_0".to_string(),
            flash_attn: true,
            gpu_layers: 99,
            mtp: true,
            spec_draft_n_max: 3,
            ngram_match: 24,
            ngram_min: 8,
            ngram_max: 32,
            extra_args: String::new(),
        }
    }
}

impl LaunchInput {
    /// Refuses what would make a broken or unsafe command line. The arguments are never passed
    /// through a shell, but a stray newline or NUL has no business in one either.
    pub fn validate(&self) -> Result<(), LaunchStoreErrors> {
        let invalid = |why: &str| Err(LaunchStoreErrors::Invalid(why.to_string()));
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return invalid("a profile needs a name of up to 80 characters");
        }
        if let Some(file) = &self.mmproj_file {
            if !is_safe_relative_path(file) {
                return invalid("the projector must be a file inside the model folder");
            }
        }
        if self.context_length.is_some_and(|c| !(512..=4_194_304).contains(&c)) {
            return invalid("the context length must be between 512 and 4194304 tokens, or left empty to size it to free memory");
        }
        for (label, value) in [("K", &self.cache_type_k), ("V", &self.cache_type_v)] {
            if !CACHE_TYPES.contains(&value.as_str()) {
                return Err(LaunchStoreErrors::Invalid(format!(
                    "the {label} cache type must be one of {}",
                    CACHE_TYPES.join(", ")
                )));
            }
        }
        if !(0..=999).contains(&self.gpu_layers) {
            return invalid("the GPU layer count must be between 0 and 999");
        }
        if !(1..=16).contains(&self.spec_draft_n_max) {
            return invalid("the draft length must be between 1 and 16 tokens");
        }
        if self.ngram_min < 1 || self.ngram_match < self.ngram_min || self.ngram_max < self.ngram_match {
            return invalid("the n-gram sizes must satisfy 1 <= min <= match <= max");
        }
        if self.extra_args.len() > 500 || self.extra_args.chars().any(|c| c.is_control()) {
            return invalid("extra arguments must be one line of up to 500 characters");
        }
        Ok(())
    }
}

/// A path relative to the model folder that can't climb out of it.
fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains(':')
        && !path.chars().any(|c| c.is_control())
        && path.split(['/', '\\']).all(|part| part != ".." && part != ".")
}

/// The loads of a stretch of time: how many, and the middle one's length
pub struct LoadSummary {
    pub count: u64,
    pub median_ms: Option<i64>,
}

impl LaunchStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Records that the model server was started under `profile_id` (a profile of `model_id`) and was
    /// ready `duration_ms` later.
    pub async fn record_load(&self, model_id: i64, profile_id: i64, duration_ms: i64) -> Result<(), LaunchStoreErrors> {
        model_loads::ActiveModel {
            model_id: Set(Some(model_id)),
            profile_id: Set(Some(profile_id)),
            duration_ms: Set(duration_ms),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(())
    }

    /// The loads that started at or after `since`, server-wide.
    pub async fn loads_since(&self, since: DateTimeUtc) -> Result<LoadSummary, LaunchStoreErrors> {
        let mut durations: Vec<i64> = model_loads::Entity::find()
            .filter(model_loads::Column::StartedAt.gte(since))
            .select_only()
            .column(model_loads::Column::DurationMs)
            .into_tuple()
            .all(&self.db)
            .await?;
        durations.sort_unstable();
        // The lower middle of an even count, like the other medians in the stats (nearest rank)
        let median_ms = (!durations.is_empty()).then(|| durations[(durations.len() - 1) / 2]);
        Ok(LoadSummary { count: durations.len() as u64, median_ms })
    }

    /// Every profile, or one model's, oldest first within each model.
    pub async fn list(&self, model_id: Option<i64>) -> Result<Vec<LaunchProfile>, LaunchStoreErrors> {
        let mut query = launch_profiles::Entity::find();
        if let Some(model_id) = model_id {
            query = query.filter(launch_profiles::Column::ModelId.eq(model_id));
        }
        Ok(query
            .order_by_asc(launch_profiles::Column::ModelId)
            .order_by_asc(launch_profiles::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn get(&self, id: i64) -> Result<LaunchProfile, LaunchStoreErrors> {
        launch_profiles::Entity::find_by_id(id)
            .one(&self.db)
            .await?
            .map(Into::into)
            .ok_or(LaunchStoreErrors::NotFound)
    }

    /// The profile new work on `model_id` starts on: the one a user picked (`chosen`) when it is really
    /// one of this model's, else the model's oldest.
    pub async fn start_profile(&self, model_id: i64, chosen: Option<i64>) -> Result<Option<LaunchProfile>, LaunchStoreErrors> {
        if let Some(id) = chosen {
            match self.get(id).await {
                Ok(profile) if profile.model_id == model_id => return Ok(Some(profile)),
                // A profile that is gone or belongs to another model is no choice at all
                Ok(_) | Err(LaunchStoreErrors::NotFound) => {}
                Err(e) => return Err(e),
            }
        }
        self.default_for_model(model_id).await
    }

    /// The profile a model starts with when nothing says which: its oldest one.
    pub async fn default_for_model(&self, model_id: i64) -> Result<Option<LaunchProfile>, LaunchStoreErrors> {
        Ok(launch_profiles::Entity::find()
            .filter(launch_profiles::Column::ModelId.eq(model_id))
            .order_by_asc(launch_profiles::Column::Id)
            .one(&self.db)
            .await?
            .map(Into::into))
    }

    pub async fn create(&self, model_id: i64, input: LaunchInput) -> Result<LaunchProfile, LaunchStoreErrors> {
        input.validate()?;
        let row = launch_profiles::ActiveModel {
            model_id: Set(model_id),
            name: Set(input.name.trim().to_string()),
            mmproj_file: Set(input.mmproj_file),
            mmproj_gpu: Set(input.mmproj_gpu),
            context_length: Set(input.context_length),
            cache_type_k: Set(input.cache_type_k),
            cache_type_v: Set(input.cache_type_v),
            flash_attn: Set(input.flash_attn),
            gpu_layers: Set(input.gpu_layers),
            mtp: Set(input.mtp),
            spec_draft_n_max: Set(input.spec_draft_n_max),
            ngram_match: Set(input.ngram_match),
            ngram_min: Set(input.ngram_min),
            ngram_max: Set(input.ngram_max),
            extra_args: Set(input.extra_args.trim().to_string()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(map_write_error)?;
        Ok(row.into())
    }

    pub async fn update(&self, id: i64, input: LaunchInput) -> Result<LaunchProfile, LaunchStoreErrors> {
        input.validate()?;
        self.get(id).await?;
        let row = launch_profiles::ActiveModel {
            id: Set(id),
            name: Set(input.name.trim().to_string()),
            mmproj_file: Set(input.mmproj_file),
            mmproj_gpu: Set(input.mmproj_gpu),
            context_length: Set(input.context_length),
            cache_type_k: Set(input.cache_type_k),
            cache_type_v: Set(input.cache_type_v),
            flash_attn: Set(input.flash_attn),
            gpu_layers: Set(input.gpu_layers),
            mtp: Set(input.mtp),
            spec_draft_n_max: Set(input.spec_draft_n_max),
            ngram_match: Set(input.ngram_match),
            ngram_min: Set(input.ngram_min),
            ngram_max: Set(input.ngram_max),
            extra_args: Set(input.extra_args.trim().to_string()),
            updated_at: Set(chrono::Utc::now()),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .map_err(map_write_error)?;
        Ok(row.into())
    }

    /// Removes a profile. A chat on it keeps its model and falls back to the model's default
    /// profile (`chats.launch_profile_id` is set to NULL), so no chat is lost.
    pub async fn delete(&self, id: i64) -> Result<(), LaunchStoreErrors> {
        let deleted = launch_profiles::Entity::delete_by_id(id).exec(&self.db).await?;
        if deleted.rows_affected == 0 {
            return Err(LaunchStoreErrors::NotFound);
        }
        Ok(())
    }
}

fn map_write_error(e: DbErr) -> LaunchStoreErrors {
    match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(detail)) if detail.contains(NAME_UNIQUE) => LaunchStoreErrors::Duplicate,
        Some(SqlErr::ForeignKeyConstraintViolation(_)) => LaunchStoreErrors::NoSuchModel,
        _ => LaunchStoreErrors::QueryFailed(e),
    }
}

#[derive(Debug)]
pub enum LaunchStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    /// A profile of that name already exists for the model
    Duplicate,
    NoSuchModel,
    Invalid(String),
}

impl From<DbErr> for LaunchStoreErrors {
    fn from(err: DbErr) -> Self {
        LaunchStoreErrors::QueryFailed(err)
    }
}

impl From<LaunchStoreErrors> for ErrorService {
    fn from(err: LaunchStoreErrors) -> Self {
        match err {
            LaunchStoreErrors::QueryFailed(e) => {
                tracing::error!("launch store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            LaunchStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "no such launch profile"),
            LaunchStoreErrors::Duplicate => {
                ErrorService::new(StatusCode::CONFLICT, "this model already has a launch profile with that name")
            }
            LaunchStoreErrors::NoSuchModel => ErrorService::new(StatusCode::NOT_FOUND, "no such model"),
            LaunchStoreErrors::Invalid(why) => ErrorService::new(StatusCode::BAD_REQUEST, why),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_valid() {
        assert!(LaunchInput::default().validate().is_ok());
    }

    #[test]
    fn what_could_break_a_command_line_is_refused() {
        let bad = |edit: fn(&mut LaunchInput)| {
            let mut input = LaunchInput::default();
            edit(&mut input);
            input.validate().is_err()
        };
        assert!(bad(|i| i.name = "  ".into()));
        assert!(bad(|i| i.mmproj_file = Some("../secret.gguf".into())));
        assert!(bad(|i| i.mmproj_file = Some("/etc/passwd".into())));
        assert!(bad(|i| i.mmproj_file = Some("C:\\x.gguf".into())));
        assert!(bad(|i| i.context_length = Some(100)));
        assert!(bad(|i| i.cache_type_k = "q8_0; rm -rf /".into()));
        assert!(bad(|i| i.gpu_layers = -1));
        assert!(bad(|i| i.spec_draft_n_max = 0));
        assert!(bad(|i| i.ngram_match = 4));
        assert!(bad(|i| i.extra_args = "--x\n--y".into()));
        let mut ok = LaunchInput::default();
        ok.mmproj_file = Some("vision/mmproj-F16.gguf".into());
        ok.context_length = Some(131072);
        ok.extra_args = "--no-mmap --threads 6".into();
        assert!(ok.validate().is_ok());
    }
}
