use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use serde_json::{Value, json};

use super::store::SkillStore;
use crate::plugins::base::{Plugin, PluginError};

/// Skills the model can read when a task matches one: each user's own set, kept by the shared `SkillStore`. What the model
/// sees of them is their list in the system prompt (`prompt_section`); the rest is read through tools on request.
pub struct SkillsPlugin {
    pub(super) store: Arc<SkillStore>,
}

#[async_trait]
impl Plugin for SkillsPlugin {
    fn plugin_name(&self) -> &str {
        "knowledge"
    }

    fn plugin_subname(&self) -> &str {
        "skills"
    }

    fn settings_value(&self) -> Value {
        json!({})
    }

    fn api_router(&self) -> Router {
        Router::new()
    }

    async fn on_enabled(&self) -> Result<(), PluginError> {
        Ok(())
    }

    async fn on_disabled(&self) -> Result<(), PluginError> {
        Ok(())
    }

    /// The user's skills, one line each, in name order so the text is the same until a skill is added, changed or deleted
    /// (it sits in the prompt every turn). Left out when the chat has no tools, since the skills could not be read, and
    /// when the user has none.
    async fn prompt_section(&self, user_id: i64, tools: bool) -> Option<String> {
        if !tools {
            return None;
        }
        // A list that cannot be read is left out of this prompt rather than failing the chat
        let skills = match self.store.list(user_id).await {
            Ok(skills) => skills,
            Err(e) => {
                tracing::warn!("skills for user {user_id} could not be listed for the prompt: {e}");
                return None;
            }
        };
        if skills.is_empty() {
            return None;
        }
        let mut text = String::from(
            "# Skills\nBefore starting a task, check the list below for a skill whose description matches it; \
             if one does, read it with skills.read and follow it.\n",
        );
        for skill in skills {
            // A description is one line in the list, however it was written
            let description = skill.description.split_whitespace().collect::<Vec<_>>().join(" ");
            text.push_str(&format!("- {}: {description}\n", skill.name));
        }
        Some(text.trim_end().to_string())
    }
}
