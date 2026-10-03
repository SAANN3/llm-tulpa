use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::launch_store::{LaunchInput, LaunchProfile};

/// A launch profile as the API shows it.
#[derive(Serialize, ToSchema)]
pub(crate) struct LaunchProfileOut {
    pub(crate) id: i64,
    pub(crate) model_id: i64,
    /// The model's identity (a file name for llama.cpp) and its provider, so a list needs no join.
    pub(crate) model: String,
    pub(crate) provider: String,
    pub(crate) name: String,
    pub(crate) mmproj_file: Option<String>,
    /// Whether the projector runs on the GPU or, when `false`, the CPU
    pub(crate) mmproj_gpu: bool,
    /// `null` sizes the context to free memory
    pub(crate) context_length: Option<i32>,
    pub(crate) cache_type_k: String,
    pub(crate) cache_type_v: String,
    pub(crate) flash_attn: bool,
    pub(crate) gpu_layers: i32,
    pub(crate) mtp: bool,
    pub(crate) spec_draft_n_max: i32,
    pub(crate) ngram_match: i32,
    pub(crate) ngram_min: i32,
    pub(crate) ngram_max: i32,
    pub(crate) extra_args: String,
}

impl LaunchProfileOut {
    pub(crate) fn new(profile: LaunchProfile, model: String, provider: String) -> Self {
        Self {
            id: profile.id,
            model_id: profile.model_id,
            model,
            provider,
            name: profile.name,
            mmproj_file: profile.mmproj_file,
            mmproj_gpu: profile.mmproj_gpu,
            context_length: profile.context_length,
            cache_type_k: profile.cache_type_k,
            cache_type_v: profile.cache_type_v,
            flash_attn: profile.flash_attn,
            gpu_layers: profile.gpu_layers,
            mtp: profile.mtp,
            spec_draft_n_max: profile.spec_draft_n_max,
            ngram_match: profile.ngram_match,
            ngram_min: profile.ngram_min,
            ngram_max: profile.ngram_max,
            extra_args: profile.extra_args,
        }
    }
}

/// What a request sets on a profile. On create, a field left out takes the default; on update it
/// keeps its current value, except `mmproj_file` and `context_length`, which are replaced as sent
/// (`null` clears them: no projector, and a context sized to free memory).
#[derive(Deserialize, ToSchema)]
pub(crate) struct LaunchProfileIn {
    pub(crate) name: Option<String>,
    pub(crate) mmproj_file: Option<String>,
    pub(crate) mmproj_gpu: Option<bool>,
    pub(crate) context_length: Option<i32>,
    pub(crate) cache_type_k: Option<String>,
    pub(crate) cache_type_v: Option<String>,
    pub(crate) flash_attn: Option<bool>,
    pub(crate) gpu_layers: Option<i32>,
    pub(crate) mtp: Option<bool>,
    pub(crate) spec_draft_n_max: Option<i32>,
    pub(crate) ngram_match: Option<i32>,
    pub(crate) ngram_min: Option<i32>,
    pub(crate) ngram_max: Option<i32>,
    pub(crate) extra_args: Option<String>,
}

impl LaunchProfileIn {
    /// The settings this request describes, with whatever it left out taken from `base`.
    pub(crate) fn over(self, base: LaunchInput) -> LaunchInput {
        LaunchInput {
            name: self.name.unwrap_or(base.name),
            mmproj_file: self.mmproj_file,
            mmproj_gpu: self.mmproj_gpu.unwrap_or(base.mmproj_gpu),
            context_length: self.context_length,
            cache_type_k: self.cache_type_k.unwrap_or(base.cache_type_k),
            cache_type_v: self.cache_type_v.unwrap_or(base.cache_type_v),
            flash_attn: self.flash_attn.unwrap_or(base.flash_attn),
            gpu_layers: self.gpu_layers.unwrap_or(base.gpu_layers),
            mtp: self.mtp.unwrap_or(base.mtp),
            spec_draft_n_max: self.spec_draft_n_max.unwrap_or(base.spec_draft_n_max),
            ngram_match: self.ngram_match.unwrap_or(base.ngram_match),
            ngram_min: self.ngram_min.unwrap_or(base.ngram_min),
            ngram_max: self.ngram_max.unwrap_or(base.ngram_max),
            extra_args: self.extra_args.unwrap_or(base.extra_args),
        }
    }
}

impl From<LaunchProfile> for LaunchInput {
    fn from(p: LaunchProfile) -> Self {
        Self {
            name: p.name,
            mmproj_file: p.mmproj_file,
            mmproj_gpu: p.mmproj_gpu,
            context_length: p.context_length,
            cache_type_k: p.cache_type_k,
            cache_type_v: p.cache_type_v,
            flash_attn: p.flash_attn,
            gpu_layers: p.gpu_layers,
            mtp: p.mtp,
            spec_draft_n_max: p.spec_draft_n_max,
            ngram_match: p.ngram_match,
            ngram_min: p.ngram_min,
            ngram_max: p.ngram_max,
            extra_args: p.extra_args,
        }
    }
}
