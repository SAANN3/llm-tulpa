use serde::{Deserialize, Serialize};

/// Overrides owned by the runtime of a service chat, never by global user settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentPolicy {
    pub system_prompt: Option<String>,
    pub write_notes: bool,
    pub write_memory: bool,
}
impl AgentPolicy {
    pub fn allows_tool(&self, name: &str) -> bool {
        self.write_notes || name != "chat.write_notes"
    }
}
