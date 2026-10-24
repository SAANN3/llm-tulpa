//! The built-in skills plugin written as an external one: folders of instructions the model reads when a task matches, one
//! set per user. The table is in the plugin's own database schema (`migrations/`), the files in its data folder, the tab
//! and the tools are the plugin's own, and nothing of the backend's code is used: it shows what a plugin with a database,
//! uploads, a prompt section and folder approvals needs.

mod format;
mod pack;
mod paths;
mod store;
mod tab;
mod tools;

use std::sync::{Arc, OnceLock};

use tulpa_plugin::{CallContext, Failure, Host, Plugin, Tab, ToolSpec, UploadedFile, Value, async_trait};

use store::SkillStore;

#[derive(Default)]
struct Skills {
    /// Made on the first `configure` and kept across the ones after, so its write lock is shared across them
    store: OnceLock<Arc<SkillStore>>,
}

impl Skills {
    fn store(&self) -> Result<&Arc<SkillStore>, Failure> {
        self.store.get().ok_or_else(|| Failure::from("the plugin has not been configured yet"))
    }
}

#[async_trait]
impl Plugin for Skills {
    async fn configure(&self, _settings: Value, host: Host) -> Result<(), Failure> {
        if self.store.get().is_none() {
            let root = host.data_dir().ok_or_else(|| Failure::from("the backend did not say where the plugin's data folder is"))?;
            let _ = self.store.set(Arc::new(SkillStore::new(host, root)));
        }
        Ok(())
    }

    fn tools(&self) -> Vec<ToolSpec> {
        tools::specs()
    }

    async fn call_tool(&self, tool: &str, args: Value, ctx: CallContext) -> Result<Value, Failure> {
        tools::call(self.store()?, ctx.user_id, tool, args).await
    }

    /// The user's skills, one line each, in name order so the text is the same until a skill is added, changed or deleted
    /// (it sits in the prompt every turn). Left out when the chat has no tools, since the skills could not be read, and
    /// when the user has none.
    async fn prompt_section(&self, user_id: i64, tools: bool) -> Option<String> {
        if !tools {
            return None;
        }
        // A list that cannot be read is left out of this prompt rather than failing the chat
        let skills = match self.store().ok()?.list(user_id).await {
            Ok(skills) => skills,
            Err(e) => {
                eprintln!("skills for user {user_id} could not be listed for the prompt: {e}");
                return None;
            }
        };
        let skills: Vec<_> = skills.into_iter().filter(|skill| skill.enabled).collect();
        if skills.is_empty() {
            return None;
        }
        let mut text = String::from(
            "# Skills\nBefore starting a task, check the list below for a skill whose description matches it; \
             if one does, read it with example_skills.read and follow it.\n",
        );
        for skill in skills {
            text.push_str(&format!("- {}: {}\n", skill.name, skill.description));
        }
        Some(text.trim_end().to_string())
    }

    async fn tab_layout(&self, user_id: i64) -> Result<Tab, Failure> {
        tab::layout(self.store()?, user_id, None, None).await
    }

    async fn tab_action(&self, user_id: i64, action: &str, args: Value) -> Result<Tab, Failure> {
        tab::action(self.store()?, user_id, action, args).await
    }

    async fn tab_upload(&self, user_id: i64, action: &str, row: Option<String>, files: Vec<UploadedFile>) -> Result<Tab, Failure> {
        tab::upload(self.store()?, user_id, action, row, files).await
    }

    async fn user_deleted(&self, user_id: i64) {
        let Ok(store) = self.store() else { return };
        if let Err(e) = store.remove_user_files(user_id).await {
            eprintln!("skills of deleted user {user_id} could not be removed: {e}");
        }
    }
}

fn main() -> Result<(), tulpa_plugin::Error> {
    tulpa_plugin::serve(Skills::default())
}
