use super::{
    definition::{Input, Literal, ModelChoice},
    execution::{MAX_VALUE_BYTES, Outcome, Values},
    runs::RunState,
};
use crate::{
    facade::{launch::LaunchFacade, one_shot::OneShot},
    services::{
        error::ErrorService,
        launch_store::LaunchStore,
        llm::{ChatMessage, LlmProviders, ThinkChoice},
        model_store::ModelStore,
    },
};
use std::{collections::BTreeMap, sync::Arc};
use tokio_util::sync::CancellationToken;

pub(crate) struct Generation {
    models: Arc<ModelStore>,
    profiles: Arc<LaunchStore>,
    launch: Arc<LaunchFacade>,
    providers: LlmProviders,
    one_shot: OneShot,
}
impl Generation {
    pub fn new(
        models: Arc<ModelStore>,
        profiles: Arc<LaunchStore>,
        launch: Arc<LaunchFacade>,
        providers: LlmProviders,
    ) -> Self {
        let one_shot = OneShot::new(providers.clone());
        Self { models, profiles, launch, providers, one_shot }
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
        let (model, profile_id) = match choice {
            ModelChoice::UserDefault => {
                let model = self.models.resolve_default(user_id).await?;
                let profile = self.profiles.start_profile(model.id, self.models.chosen_profile(user_id).await?).await?;
                (model, profile.map(|p| p.id))
            }
            ModelChoice::Model { id, launch_profile_id } => {
                let model = self.models.get_many(&[*id]).await?.remove(id)
                    .ok_or_else(|| ErrorService::internal("generation model is no longer registered"))?;
                if let Some(id) = launch_profile_id {
                    let profile = self.profiles.get(*id).await?;
                    if profile.model_id != model.id {
                        return Err(ErrorService::internal("launch profile belongs to another model"));
                    }
                }
                (model, *launch_profile_id)
            }
        };
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
            port: Some("out".into()),
            outputs: BTreeMap::from([("text".into(), Literal::String(text))]),
        })
    }
}
