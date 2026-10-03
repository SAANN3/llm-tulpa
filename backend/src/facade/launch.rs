//! Turns "this user, this model, this launch profile" into the `LaunchRequest` the model server is
//! asked to run — the join between the stores that know the pieces and the runtime that starts a
//! process from them.

use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::services::{
    error::ErrorService,
    launch_store::LaunchStore,
    llama_runtime::LlamaRuntime,
    llm::{CallParams, ChatMessage, LaunchRequest, LlmProviders, ThinkChoice},
    model_store::{ModelRef, ModelStore},
    user_store::UserStore,
};

/// The provider whose server the backend starts itself.
pub const MANAGED_PROVIDER: &str = "llama-cpp";

pub struct LaunchFacade {
    providers: LlmProviders,
    runtime: Arc<LlamaRuntime>,
    launch: Arc<LaunchStore>,
    models: Arc<ModelStore>,
    users: Arc<UserStore>,
    model_dir: Option<PathBuf>,
}

impl LaunchFacade {
    pub fn new(
        providers: LlmProviders,
        runtime: Arc<LlamaRuntime>,
        launch: Arc<LaunchStore>,
        models: Arc<ModelStore>,
        users: Arc<UserStore>,
        model_dir: Option<PathBuf>,
    ) -> Self {
        Self { providers, runtime, launch, models, users, model_dir }
    }

    /// What `user_id` needs running to use `model_id` under `profile_id` (the model's default profile
    /// when none is given). `None` for a model whose server isn't started by the backend, or one with
    /// no profile yet: those are used as they are.
    pub async fn request_for(&self, user_id: i64, model_id: i64, profile_id: Option<i64>) -> Result<Option<LaunchRequest>, ErrorService> {
        let Some(model) = self.models.get_many(&[model_id]).await?.remove(&model_id) else { return Ok(None) };
        self.request_for_model(user_id, &model, profile_id).await
    }

    /// The same for a model already in hand.
    pub async fn request_for_model(&self, user_id: i64, model: &ModelRef, profile_id: Option<i64>) -> Result<Option<LaunchRequest>, ErrorService> {
        let (Some(dir), true) = (&self.model_dir, model.provider == MANAGED_PROVIDER) else { return Ok(None) };
        let profile = match profile_id {
            Some(id) => self.launch.get(id).await?,
            None => match self.launch.default_for_model(model.id).await? {
                Some(profile) => profile,
                None => return Ok(None),
            },
        };
        // Said in the other user's "in use by ..." message, so a missing user is no reason to fail.
        let holder = self.users.get(user_id).await.ok().flatten().map(|u| u.username).unwrap_or_else(|| format!("user {user_id}"));
        Ok(Some(LaunchRequest { model_file: dir.join(&model.name), model_root: dir.clone(), profile, holder }))
    }

    /// The request for running `profile_id` on behalf of `user_id`.
    pub async fn request_for_profile(&self, user_id: i64, profile_id: i64) -> Result<LaunchRequest, ErrorService> {
        let profile = self.launch.get(profile_id).await?;
        self.request_for(user_id, profile.model_id, Some(profile_id)).await?.ok_or_else(|| {
            ErrorService::new(StatusCode::CONFLICT, "this model isn't one the backend runs itself, or no model directory is configured")
        })
    }

    /// Loads `profile_id` and sends one short prompt through it, to measure what it really does:
    /// how fast it reads a prompt and writes a reply, and how often MTP drafting was right.
    pub async fn speed_test(&self, user_id: i64, profile_id: i64, prompt: String) -> Result<SpeedTest, ErrorService> {
        let request = self.request_for_profile(user_id, profile_id).await?;
        let model = self.models.get_many(&[request.profile.model_id]).await?.remove(&request.profile.model_id);
        let model_name = model.map(|m| m.name).unwrap_or_default();
        let provider = self.providers.get(MANAGED_PROVIDER)?;

        // Loaded first: a load clears the log, so the position to read from is taken once it is up
        let _claim = self.runtime.acquire(Some(&request)).await?;
        let before = self.runtime.log_position();
        let params = CallParams { launch: Some(request), ..CallParams::default() };
        let response = provider
            .chat(vec![], Some(ChatMessage::user(prompt)), &[], Some(ThinkChoice::Enabled(false)), &model_name, None, &params)
            .await?;

        let tokens_per_second = |count: Option<u64>, ms: Option<i64>| match (count, ms) {
            (Some(count), Some(ms)) if ms > 0 => Some(count as f64 / (ms as f64 / 1000.0)),
            _ => None,
        };
        // The server prints the drafting score after the request finishes, a moment later than the reply.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let (_, lines) = self.runtime.logs(Some(before), 200);
        Ok(SpeedTest {
            reply: response.message.content.clone(),
            prompt_tokens: response.prompt_eval_count(),
            generated_tokens: response.eval_count(),
            prompt_tokens_per_second: tokens_per_second(response.prompt_processed_tokens().map(|n| n as u64), response.prompt_eval_duration_ms()),
            generated_tokens_per_second: tokens_per_second(response.eval_count(), response.eval_duration_ms()),
            draft_acceptance: lines.iter().rev().find_map(|line| draft_acceptance(line)),
        })
    }

    /// The context window each launch profile gives, for working out how much of it a chat has used.
    /// `fallback` is the configured window, which is what a chat with no profile (an Ollama model)
    /// runs under.
    pub async fn contexts(&self, fallback: u64) -> Result<ContextBook, ErrorService> {
        let loaded = self.runtime.status();
        let by_profile = self
            .launch
            .list(None)
            .await?
            .into_iter()
            .filter_map(|profile| {
                let context = match profile.context_length {
                    Some(context) => Some(context as u64),
                    // Sized by the server to free memory: known only while that profile is loaded.
                    None if loaded.profile_id == Some(profile.id) => loaded.facts.context_per_slot,
                    None => None,
                };
                context.map(|context| (profile.id, context))
            })
            .collect();
        Ok(ContextBook { by_profile, fallback })
    }

    /// What the owner's default model would run under: the thing started when the backend boots and
    /// the thing a call that names no launch falls back to when nothing is loaded.
    pub async fn default_request(&self) -> Result<Option<LaunchRequest>, ErrorService> {
        let Some(owner) = self.users.owner_id().await? else { return Ok(None) };
        self.default_request_for(owner).await
    }

    /// What `user_id`'s default model would run under.
    pub async fn default_request_for(&self, user_id: i64) -> Result<Option<LaunchRequest>, ErrorService> {
        let Some(model) = self.models.default_for_user(user_id).await? else { return Ok(None) };
        self.request_for_model(user_id, &model, None).await
    }
}

/// The context window of each launch profile, see `LaunchFacade::contexts`.
pub struct ContextBook {
    by_profile: std::collections::HashMap<i64, u64>,
    fallback: u64,
}

impl ContextBook {
    pub fn for_profile(&self, profile_id: Option<i64>) -> u64 {
        profile_id.and_then(|id| self.by_profile.get(&id).copied()).unwrap_or(self.fallback)
    }
}

/// What a test prompt measured.
#[derive(Serialize, ToSchema)]
pub struct SpeedTest {
    pub reply: String,
    pub prompt_tokens: Option<u64>,
    pub generated_tokens: Option<u64>,
    /// `None` when the server read the prompt from its cache and so processed (almost) nothing
    pub prompt_tokens_per_second: Option<f64>,
    pub generated_tokens_per_second: Option<f64>,
    /// The share of drafted tokens the model accepted (0 to 1); `None` without MTP drafting
    pub draft_acceptance: Option<f64>,
}

/// The score from a line like `draft acceptance = 0.63704 (   86 accepted /   135 generated)`.
fn draft_acceptance(line: &str) -> Option<f64> {
    let rest = line.split("acceptance rate =").nth(1).or_else(|| line.split("draft acceptance =").nth(1))?;
    rest.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::draft_acceptance;

    #[test]
    fn reads_the_drafting_score() {
        assert_eq!(draft_acceptance("draft acceptance rate = 0.79412 (  27 accepted /    34 generated)"), Some(0.79412));
        assert_eq!(draft_acceptance("slot print_timing: id 0 | task 0 | draft acceptance = 0.63704 (   86 accepted /   135 generated), mean len =  2.91"), Some(0.63704));
        assert_eq!(draft_acceptance("slot print_timing: id 0"), None);
    }
}
