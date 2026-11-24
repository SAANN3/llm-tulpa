use super::{
    definition::{Input, Literal, ModelChoice},
    execution::{MAX_VALUE_BYTES, Outcome, Values},
    runs::RunState,
};
use crate::{
    facade::{launch::LaunchFacade, one_shot::OneShot},
    services::{
        error::ErrorService,
        llm::{ChatMessage, LlmProviders, ThinkChoice},
    },
};
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;

pub(crate) struct Generation {
    launch: Arc<LaunchFacade>,
    providers: LlmProviders,
    one_shot: OneShot,
}
impl Generation {
    pub fn new(
        launch: Arc<LaunchFacade>,
        providers: LlmProviders,
    ) -> Self {
        let one_shot = OneShot::new(providers.clone());
        Self { launch, providers, one_shot }
    }

    pub(super) async fn execute(
        &self,
        user_id: i64,
        state: &RunState,
        model: &ModelChoice,
        prompt: &Input,
        think: Option<ThinkChoice>,
        cancel: &CancellationToken,
    ) -> Result<Outcome, String> {
        let mut values = Values { state, remaining: MAX_VALUE_BYTES };
        let prompt = match values.resolve(prompt, 0)? {
            Literal::String(prompt) => prompt,
            _ => return Err("generation prompt must be a string".into()),
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("cancelled".into()),
            result = self.complete(user_id, model, prompt, think) => {
                result.map_err(|e| e.message.unwrap_or_else(|| "generation failed".into()))
            }
        }
    }

    async fn complete(
        &self,
        user_id: i64,
        choice: &ModelChoice,
        prompt: String,
        think: Option<ThinkChoice>,
    ) -> Result<Outcome, ErrorService> {
        let (model_id, profile_id) = match choice {
            ModelChoice::UserDefault => (None, None),
            ModelChoice::Model { id, launch_profile_id } => (Some(*id), *launch_profile_id),
        };
        let (model, profile_id) = self.launch.model_for_call(user_id, model_id, profile_id).await?;
        let params = self.launch.call_params(user_id, model.id, profile_id).await?;
        let provider = self.providers.get(&model.provider)?;
        let _guard = provider.acquire(params.launch.as_ref()).await?;
        let response = self.one_shot.complete(
            &model.provider, &model.name, vec![ChatMessage::user(prompt)], think, &params, None,
        ).await?;
        if response.message.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()) {
            return Err(ErrorService::internal("generation returned a tool call without tool definitions"));
        }
        let text = response.message.content;
        if text.len() > MAX_VALUE_BYTES {
            return Err(ErrorService::internal("generation output exceeds 1 MiB"));
        }
        Ok(Outcome {
            assignment: None,
            control_output: Some("out".into()),
            outputs: BTreeMap::from([("text".into(), Literal::String(text))]),
        })
    }
}
