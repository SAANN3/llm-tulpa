mod entities;

use axum::http::StatusCode;
use entities::{sampling_presets, user_preset_choice};
use sea_orm::{
    prelude::*, sea_query::OnConflict, ActiveValue::Set, Condition, DatabaseConnection, DbErr, QueryOrder, SqlErr,
};

use crate::services::error::ErrorService;
use crate::services::llm::Sampling;

/// Owns a user's sampling presets: named sets of temperature, top-p and the like, kept per user
/// because how a model should sound is a matter of taste (a high-temperature one for fun, a cold
/// one for code). A preset is tied to one model or, with no model, usable with any. A user chooses
/// one preset per model (`user_preset_choice`); a value a preset leaves empty is not sent, so the
/// server's own default applies.
pub struct PresetStore {
    db: DatabaseConnection,
}

/// The longest name a preset may have
const MAX_NAME_CHARS: usize = 80;
const NAME_UNIQUE: &str = "sampling_presets_name_unique";

#[derive(Clone, Debug)]
pub struct SamplingPreset {
    pub id: i64,
    /// The model this preset is for, or `None` for any model
    pub model_id: Option<i64>,
    pub name: String,
    pub sampling: Sampling,
    /// The name of the model this preset was made for, when that model has been removed
    pub removed_model: Option<String>,
}

impl From<sampling_presets::Model> for SamplingPreset {
    fn from(m: sampling_presets::Model) -> Self {
        Self {
            id: m.id,
            model_id: m.model_id,
            name: m.name,
            removed_model: m.removed_model,
            sampling: Sampling {
                temperature: m.temperature,
                top_p: m.top_p,
                top_k: m.top_k,
                min_p: m.min_p,
                repeat_penalty: m.repeat_penalty,
                presence_penalty: m.presence_penalty,
                seed: m.seed,
            },
        }
    }
}

/// What a request may set on a preset.
#[derive(Clone, Debug)]
pub struct PresetInput {
    pub model_id: Option<i64>,
    pub name: String,
    pub sampling: Sampling,
}

impl PresetInput {
    /// Refuses values no sampler accepts, so a typo can't make every reply fail.
    pub fn validate(&self) -> Result<(), PresetStoreErrors> {
        let invalid = |why: &str| Err(PresetStoreErrors::Invalid(why.to_string()));
        let s = &self.sampling;
        if self.name.trim().is_empty() || self.name.chars().count() > MAX_NAME_CHARS {
            return invalid("a preset needs a name of up to 80 characters");
        }
        if s.temperature.is_some_and(|v| !(0.0..=5.0).contains(&v)) {
            return invalid("the temperature must be between 0 and 5");
        }
        if s.top_p.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
            return invalid("top-p must be between 0 and 1");
        }
        if s.top_k.is_some_and(|v| !(0..=1000).contains(&v)) {
            return invalid("top-k must be between 0 and 1000 (0 turns it off)");
        }
        if s.min_p.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
            return invalid("min-p must be between 0 and 1");
        }
        if s.repeat_penalty.is_some_and(|v| !(0.0..=3.0).contains(&v)) {
            return invalid("the repeat penalty must be between 0 and 3");
        }
        if s.presence_penalty.is_some_and(|v| !(-2.0..=2.0).contains(&v)) {
            return invalid("the presence penalty must be between -2 and 2");
        }
        Ok(())
    }
}

/// Starting points a user can copy into their own presets. Not the values any model's author
/// recommends, only common shapes; the final word is the user's.
pub fn templates() -> Vec<PresetInput> {
    let preset = |name: &str, sampling: Sampling| PresetInput { model_id: None, name: name.to_string(), sampling };
    vec![
        preset("Precise", Sampling { temperature: Some(0.2), top_p: Some(0.9), ..Default::default() }),
        preset("Balanced", Sampling { temperature: Some(0.7), top_p: Some(0.8), top_k: Some(20), ..Default::default() }),
        preset("Creative", Sampling { temperature: Some(1.0), top_p: Some(0.95), min_p: Some(0.05), ..Default::default() }),
        preset("Repeatable", Sampling { temperature: Some(0.0), seed: Some(1), ..Default::default() }),
    ]
}

impl PresetStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// A user's presets by name: those for `model_id` and the any-model ones, or all of them
    /// when no model is given.
    pub async fn list(&self, user_id: i64, model_id: Option<i64>) -> Result<Vec<SamplingPreset>, PresetStoreErrors> {
        let mut query = sampling_presets::Entity::find().filter(sampling_presets::Column::UserId.eq(user_id));
        if let Some(model_id) = model_id {
            query = query.filter(
                Condition::any()
                    .add(sampling_presets::Column::ModelId.eq(model_id))
                    .add(sampling_presets::Column::ModelId.is_null()),
            );
        }
        Ok(query
            .order_by_asc(sampling_presets::Column::Name)
            .all(&self.db)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// A preset of this user's, or `NotFound` — which is also what someone else's preset is.
    pub async fn owned(&self, user_id: i64, id: i64) -> Result<SamplingPreset, PresetStoreErrors> {
        sampling_presets::Entity::find_by_id(id)
            .filter(sampling_presets::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?
            .map(Into::into)
            .ok_or(PresetStoreErrors::NotFound)
    }

    pub async fn create(&self, user_id: i64, input: PresetInput) -> Result<SamplingPreset, PresetStoreErrors> {
        input.validate()?;
        let s = &input.sampling;
        let row = sampling_presets::ActiveModel {
            user_id: Set(user_id),
            model_id: Set(input.model_id),
            name: Set(input.name.trim().to_string()),
            temperature: Set(s.temperature),
            top_p: Set(s.top_p),
            top_k: Set(s.top_k),
            min_p: Set(s.min_p),
            repeat_penalty: Set(s.repeat_penalty),
            presence_penalty: Set(s.presence_penalty),
            seed: Set(s.seed),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(map_write_error)?;
        Ok(row.into())
    }

    pub async fn update(&self, user_id: i64, id: i64, input: PresetInput) -> Result<SamplingPreset, PresetStoreErrors> {
        input.validate()?;
        self.owned(user_id, id).await?;
        let s = &input.sampling;
        let row = sampling_presets::ActiveModel {
            id: Set(id),
            model_id: Set(input.model_id),
            name: Set(input.name.trim().to_string()),
            temperature: Set(s.temperature),
            top_p: Set(s.top_p),
            top_k: Set(s.top_k),
            min_p: Set(s.min_p),
            repeat_penalty: Set(s.repeat_penalty),
            presence_penalty: Set(s.presence_penalty),
            seed: Set(s.seed),
            updated_at: Set(chrono::Utc::now()),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .map_err(map_write_error)?;
        Ok(row.into())
    }

    /// Keeps every preset made for `model_id` when that model is removed: each becomes a preset for any
    /// model that says which model it was made for. A name the user already has among their any-model
    /// presets gets a number added, since two presets of one user can't share a name. Returns how many
    /// presets were kept.
    pub async fn detach_model(&self, model_id: i64, model_name: &str) -> Result<u64, PresetStoreErrors> {
        let rows = sampling_presets::Entity::find()
            .filter(sampling_presets::Column::ModelId.eq(model_id))
            .order_by_asc(sampling_presets::Column::Id)
            .all(&self.db)
            .await?;
        let mut kept = 0;
        for row in rows {
            let taken: std::collections::HashSet<String> = sampling_presets::Entity::find()
                .filter(sampling_presets::Column::UserId.eq(row.user_id))
                .filter(sampling_presets::Column::ModelId.is_null())
                .all(&self.db)
                .await?
                .into_iter()
                .map(|p| p.name)
                .collect();
            let name = unused_name(&row.name, &taken);
            sampling_presets::ActiveModel {
                id: Set(row.id),
                model_id: Set(None),
                name: Set(name),
                removed_model: Set(Some(model_name.to_string())),
                updated_at: Set(chrono::Utc::now()),
                ..Default::default()
            }
            .update(&self.db)
            .await
            .map_err(map_write_error)?;
            kept += 1;
        }
        Ok(kept)
    }

    /// Deletes a preset; a choice of it goes with it.
    pub async fn delete(&self, user_id: i64, id: i64) -> Result<(), PresetStoreErrors> {
        let deleted = sampling_presets::Entity::delete_many()
            .filter(sampling_presets::Column::Id.eq(id))
            .filter(sampling_presets::Column::UserId.eq(user_id))
            .exec(&self.db)
            .await?;
        if deleted.rows_affected == 0 {
            return Err(PresetStoreErrors::NotFound);
        }
        Ok(())
    }

    /// Makes `preset_id` the user's choice for `model_id`, or clears the choice with `None` (the
    /// server's defaults apply again). A preset made for another model can't be chosen.
    pub async fn choose(&self, user_id: i64, model_id: i64, preset_id: Option<i64>) -> Result<(), PresetStoreErrors> {
        let Some(preset_id) = preset_id else {
            user_preset_choice::Entity::delete_many()
                .filter(user_preset_choice::Column::UserId.eq(user_id))
                .filter(user_preset_choice::Column::ModelId.eq(model_id))
                .exec(&self.db)
                .await?;
            return Ok(());
        };

        let preset = self.owned(user_id, preset_id).await?;
        if preset.model_id.is_some_and(|m| m != model_id) {
            return Err(PresetStoreErrors::Invalid("that preset was made for another model".to_string()));
        }
        user_preset_choice::Entity::insert(user_preset_choice::ActiveModel {
            user_id: Set(user_id),
            model_id: Set(model_id),
            preset_id: Set(preset_id),
        })
        .on_conflict(
            OnConflict::columns([user_preset_choice::Column::UserId, user_preset_choice::Column::ModelId])
                .update_column(user_preset_choice::Column::PresetId)
                .to_owned(),
        )
        .exec(&self.db)
        .await?;
        Ok(())
    }

    /// The preset the user has chosen for the model, if any.
    pub async fn chosen(&self, user_id: i64, model_id: i64) -> Result<Option<SamplingPreset>, PresetStoreErrors> {
        let Some(choice) = user_preset_choice::Entity::find_by_id((user_id, model_id)).one(&self.db).await? else {
            return Ok(None);
        };
        Ok(sampling_presets::Entity::find_by_id(choice.preset_id).one(&self.db).await?.map(Into::into))
    }

    /// What a call for this user on this model samples with: their chosen preset's values, or
    /// nothing (the server's defaults) when they have chosen none.
    pub async fn effective_sampling(&self, user_id: i64, model_id: i64) -> Result<Sampling, PresetStoreErrors> {
        Ok(self.chosen(user_id, model_id).await?.map(|p| p.sampling).unwrap_or_default())
    }
}

fn map_write_error(e: DbErr) -> PresetStoreErrors {
    match e.sql_err() {
        Some(SqlErr::UniqueConstraintViolation(detail)) if detail.contains(NAME_UNIQUE) => PresetStoreErrors::Duplicate,
        Some(SqlErr::ForeignKeyConstraintViolation(_)) => PresetStoreErrors::NoSuchModel,
        _ => PresetStoreErrors::QueryFailed(e),
    }
}

#[derive(Debug)]
pub enum PresetStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    /// The user already has a preset of that name for the model
    Duplicate,
    NoSuchModel,
    Invalid(String),
}

impl From<DbErr> for PresetStoreErrors {
    fn from(err: DbErr) -> Self {
        PresetStoreErrors::QueryFailed(err)
    }
}

impl From<PresetStoreErrors> for ErrorService {
    fn from(err: PresetStoreErrors) -> Self {
        match err {
            PresetStoreErrors::QueryFailed(e) => {
                tracing::error!("preset store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            PresetStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "no such preset"),
            PresetStoreErrors::Duplicate => {
                ErrorService::new(StatusCode::CONFLICT, "you already have a preset with that name for this model")
            }
            PresetStoreErrors::NoSuchModel => ErrorService::new(StatusCode::NOT_FOUND, "no such model"),
            PresetStoreErrors::Invalid(why) => ErrorService::new(StatusCode::BAD_REQUEST, why),
        }
    }
}

/// `base`, or `base` with " 2", " 3"... added until it is a name nobody has; kept within the length a
/// preset name may have.
fn unused_name(base: &str, taken: &std::collections::HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| {
            let suffix = format!(" {n}");
            let room = MAX_NAME_CHARS - suffix.chars().count();
            format!("{}{suffix}", base.chars().take(room).collect::<String>())
        })
        .find(|candidate| !taken.contains(candidate))
        .expect("an unbounded range always yields an unused name")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(sampling: Sampling) -> PresetInput {
        PresetInput { model_id: None, name: "x".into(), sampling }
    }

    #[test]
    fn the_templates_are_valid() {
        for template in templates() {
            assert!(template.validate().is_ok(), "{} must validate", template.name);
        }
    }

    #[test]
    fn values_no_sampler_accepts_are_refused() {
        assert!(input(Sampling { temperature: Some(-0.1), ..Default::default() }).validate().is_err());
        assert!(input(Sampling { temperature: Some(9.0), ..Default::default() }).validate().is_err());
        assert!(input(Sampling { top_p: Some(1.5), ..Default::default() }).validate().is_err());
        assert!(input(Sampling { top_k: Some(-1), ..Default::default() }).validate().is_err());
        assert!(input(Sampling { min_p: Some(2.0), ..Default::default() }).validate().is_err());
        assert!(input(Sampling { presence_penalty: Some(3.0), ..Default::default() }).validate().is_err());
        assert!(input(Sampling::default()).validate().is_ok());
        assert!(PresetInput { name: "  ".into(), ..input(Sampling::default()) }.validate().is_err());
    }

    #[test]
    fn a_taken_name_gets_a_number_and_stays_short_enough() {
        let taken: std::collections::HashSet<String> = ["Coding", "Coding 2"].into_iter().map(String::from).collect();
        assert_eq!(unused_name("Chat", &taken), "Chat");
        assert_eq!(unused_name("Coding", &taken), "Coding 3");
        let long = "x".repeat(MAX_NAME_CHARS);
        let taken: std::collections::HashSet<String> = [long.clone()].into_iter().collect();
        let renamed = unused_name(&long, &taken);
        assert_eq!(renamed.chars().count(), MAX_NAME_CHARS);
        assert!(renamed.ends_with(" 2"));
    }
}
