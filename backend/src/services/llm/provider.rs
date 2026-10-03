//! What a model provider has to be able to do, and the registry that finds one by name.
//!
//! Every chat is bound to a model, and every model belongs to a provider (`llm_providers.name`);
//! callers look the provider up here and talk to it only through `LlmProvider`, so the rest of the
//! app never learns a provider's wire format.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;

use super::types::{
    CallParams, ChatMessage, ChatResponse, GenerateResponse, LaunchRequest, LlmErrors, LocalModel, RunningModels, ThinkChoice,
    ThinkingCapability,
};
use crate::services::error::ErrorService;
use crate::services::llama_runtime::CallGuard;
use crate::tools::base::Tool;

/// A server (or process) the backend can run a model on. No model is held by a provider: every call
/// names the one it runs against, resolved from the database by the caller.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// The `llm_providers.name` this implementation answers to.
    fn name(&self) -> &'static str;

    /// One conversation turn. `messages` is the history in order and `new_message`, when given, is
    /// appended as the turn being added now. `tools` are advertised to the model for this call and
    /// omitted when empty; running a tool the reply asks for is the caller's job. `think` defaults to
    /// enabled at the provider's own default effort. `known_prompt_tokens` is the measured prompt
    /// size of the previous call, which lets the provider budget the reply's length precisely.
    /// `params` carries what this call overrides about how the model runs it.
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        new_message: Option<ChatMessage>,
        tools: &[&dyn Tool],
        think: Option<ThinkChoice>,
        model: &str,
        known_prompt_tokens: Option<u64>,
        params: &CallParams,
    ) -> Result<ChatResponse, LlmErrors>;

    /// A one-shot completion from a bare prompt: no roles, history or tools. For the app's own
    /// short internal prompts (names, greetings, summaries).
    async fn generate(
        &self,
        prompt: String,
        think: Option<bool>,
        model: &str,
        params: &CallParams,
    ) -> Result<GenerateResponse, LlmErrors>;

    /// What the model's own chat template supports for `think`, read fresh every time so it is
    /// right the moment the model changes.
    async fn thinking_capability(&self, model: &str) -> Result<ThinkingCapability, LlmErrors>;

    /// The tags the model's template wraps a tool call in, or none when it can't be read — a failure
    /// to look is never treated as a finding.
    async fn tool_call_markers(&self, model: &str) -> Vec<String>;

    /// The models this provider can serve right now.
    async fn list_local_models(&self) -> Result<Vec<LocalModel>, LlmErrors>;

    /// What the provider has loaded and doing right now. Empty when nothing is loaded.
    async fn running_models(&self) -> Result<RunningModels, LlmErrors>;

    /// Claims the server for the caller's whole turn, starting or switching it first when the launch
    /// asks for something else than what runs. The claim lasts until the guard is dropped, and while
    /// it does another user needing a different launch is told to wait instead of reloading under
    /// the turn. A provider that runs nothing of its own has nothing to claim.
    async fn acquire(&self, launch: Option<&LaunchRequest>) -> Result<CallGuard, ErrorService> {
        let _ = launch;
        Ok(CallGuard::none())
    }
}

/// Every provider this backend has configured, by name, plus the one used when nothing says which.
#[derive(Clone)]
pub struct LlmProviders {
    by_name: Arc<HashMap<&'static str, Arc<dyn LlmProvider>>>,
    default: &'static str,
}

impl LlmProviders {
    /// `default` is registered first and answers whenever a caller has no provider to name.
    pub fn new(default: Arc<dyn LlmProvider>, others: Vec<Arc<dyn LlmProvider>>) -> Self {
        let default_name = default.name();
        let by_name = std::iter::once(default)
            .chain(others)
            .map(|provider| (provider.name(), provider))
            .collect();
        Self { by_name: Arc::new(by_name), default: default_name }
    }

    /// The provider registered under `name`. A chat whose model is on a provider this backend
    /// doesn't have configured can't run, which is a conflict with the install, not a bad request.
    pub fn get(&self, name: &str) -> Result<Arc<dyn LlmProvider>, ErrorService> {
        self.by_name.get(name).cloned().ok_or_else(|| {
            ErrorService::new(StatusCode::CONFLICT, format!("this backend has no '{name}' model provider configured"))
        })
    }

    /// The provider used when no model says otherwise.
    pub fn default_provider(&self) -> Arc<dyn LlmProvider> {
        self.by_name[self.default].clone()
    }
}
