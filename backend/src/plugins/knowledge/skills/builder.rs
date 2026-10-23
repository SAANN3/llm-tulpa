use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use serde_json::Value;

use super::plugin::SkillsPlugin;
use super::store::{self, SkillStore};
use crate::plugins::base::{Plugin, PluginBuilder, PluginError};
use crate::plugins::env::PluginEnv;
use crate::tools::base::PropertyInfo;

/// Builds the single `skills` instance. The store is made on the first build and handed to every instance built after,
/// so its write lock is shared across a rebuild.
#[derive(Default)]
pub struct SkillsBuilder {
    store: OnceLock<Arc<SkillStore>>,
}

impl SkillsBuilder {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PluginBuilder for SkillsBuilder {
    fn plugin_name(&self) -> &str {
        "knowledge"
    }

    fn plugin_subname(&self) -> &str {
        "skills"
    }

    fn settings_schema(&self) -> Vec<PropertyInfo> {
        vec![]
    }

    fn summary(&self) -> &str {
        "Folders of instructions the model reads when a task matches, one set per user"
    }

    fn help_message(&self) -> String {
        "A skill is a folder with a SKILL.md that says what it is for and how to do it, plus any files it refers to. \
         The model sees each skill's name and description in its instructions and reads the whole skill when a task \
         matches. Nothing to set up: skills are kept in the plugins folder (plugins_dir in settings.json)."
            .to_string()
    }

    fn migrations(&self, prefix: &str) -> Vec<String> {
        store::migrations(prefix)
    }

    async fn build(&self, _settings: Value, env: &PluginEnv) -> Result<Arc<dyn Plugin>, PluginError> {
        let store = self.store.get_or_init(|| Arc::new(SkillStore::new(env.db.clone(), env.data_dir.clone())));
        Ok(Arc::new(SkillsPlugin { store: store.clone() }))
    }
}
