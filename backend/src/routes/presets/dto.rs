use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::{
    llm::Sampling,
    preset_store::{PresetInput, SamplingPreset},
};

/// A sampling preset as the API shows it. A `null` value is not sent to the model, so the server's
/// own default applies.
#[derive(Serialize, ToSchema)]
pub(crate) struct PresetOut {
    pub(crate) id: i64,
    /// The model it was made for, or `null` for any model
    pub(crate) model_id: Option<i64>,
    pub(crate) name: String,
    pub(crate) temperature: Option<f32>,
    pub(crate) top_p: Option<f32>,
    pub(crate) top_k: Option<i32>,
    pub(crate) min_p: Option<f32>,
    pub(crate) repeat_penalty: Option<f32>,
    pub(crate) presence_penalty: Option<f32>,
    pub(crate) seed: Option<i64>,
    /// The name of the model it was made for, when that model has since been removed
    pub(crate) removed_model: Option<String>,
}

impl From<SamplingPreset> for PresetOut {
    fn from(p: SamplingPreset) -> Self {
        Self {
            id: p.id,
            model_id: p.model_id,
            name: p.name,
            temperature: p.sampling.temperature,
            top_p: p.sampling.top_p,
            top_k: p.sampling.top_k,
            min_p: p.sampling.min_p,
            repeat_penalty: p.sampling.repeat_penalty,
            presence_penalty: p.sampling.presence_penalty,
            seed: p.sampling.seed,
            removed_model: p.removed_model,
        }
    }
}

/// What a request sets on a preset: replaced as a whole, so a value left out (or `null`) is unset.
#[derive(Deserialize, Serialize, ToSchema, Clone)]
pub(crate) struct PresetIn {
    pub(crate) model_id: Option<i64>,
    pub(crate) name: String,
    pub(crate) temperature: Option<f32>,
    pub(crate) top_p: Option<f32>,
    pub(crate) top_k: Option<i32>,
    pub(crate) min_p: Option<f32>,
    pub(crate) repeat_penalty: Option<f32>,
    pub(crate) presence_penalty: Option<f32>,
    pub(crate) seed: Option<i64>,
}

impl From<PresetIn> for PresetInput {
    fn from(p: PresetIn) -> Self {
        PresetInput {
            model_id: p.model_id,
            name: p.name,
            sampling: Sampling {
                temperature: p.temperature,
                top_p: p.top_p,
                top_k: p.top_k,
                min_p: p.min_p,
                repeat_penalty: p.repeat_penalty,
                presence_penalty: p.presence_penalty,
                seed: p.seed,
            },
        }
    }
}

impl From<PresetInput> for PresetIn {
    fn from(p: PresetInput) -> Self {
        Self {
            model_id: p.model_id,
            name: p.name,
            temperature: p.sampling.temperature,
            top_p: p.sampling.top_p,
            top_k: p.sampling.top_k,
            min_p: p.sampling.min_p,
            repeat_penalty: p.sampling.repeat_penalty,
            presence_penalty: p.sampling.presence_penalty,
            seed: p.sampling.seed,
        }
    }
}
