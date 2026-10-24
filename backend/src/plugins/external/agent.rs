//! `agent.reply`: what a messaging plugin needs of the backend besides its own platform's API. The plugin gets a message from
//! somewhere (a chat app, a webhook), hands the text to the plugin's own agent, a model with no tools that keeps a chat per
//! conversation, and sends the answer back where the message came from.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::rpc::{BAD_ARGS, INTERNAL, RpcError};
use crate::facade::agent::Agent;
use crate::plugins::messaging::plugin::normalize_images;
use crate::services::chat_store::ChatStore;
use crate::services::llm::ThinkChoice;

const MAX_TEXT_CHARS: usize = 200_000;
const MAX_IMAGES: usize = 10;
const MAX_CONVERSATION_CHARS: usize = 200;
const MAX_TITLE_CHARS: usize = 200;

/// What the backend lends to the plugins that ask for the agent
pub struct AgentAccess {
    pub agent: Arc<Agent>,
    pub chat_store: Arc<ChatStore>,
}

/// One plugin's use of the agent: the chats it makes are filed under the plugin's own name, so two plugins (or a plugin and a
/// built-in one) never share a conversation
pub struct PluginAgent {
    access: Arc<AgentAccess>,
    plugin_name: String,
    plugin_subname: String,
}

#[derive(Deserialize)]
struct Reply {
    /// Which conversation this message is in, as the plugin names it (a chat id of the platform): the same one always gets the
    /// same chat, which is how the model remembers what was said before
    conversation: String,
    /// The name a new conversation's chat is given
    #[serde(default)]
    title: Option<String>,
    text: String,
    /// Base64 images, without a data-URL prefix
    #[serde(default)]
    images: Vec<String>,
    /// Whether the model thinks before it answers
    #[serde(default)]
    think: bool,
}

impl PluginAgent {
    pub fn new(access: Arc<AgentAccess>, plugin_name: &str, plugin_subname: &str) -> Self {
        Self { access, plugin_name: plugin_name.to_string(), plugin_subname: plugin_subname.to_string() }
    }

    /// Stores the message in the conversation's chat (made the first time), has the model answer it, and gives the answer
    pub async fn reply(&self, params: Value) -> Result<Value, RpcError> {
        let bad = |why: String| RpcError::new(BAD_ARGS, format!("agent.reply: {why}"));
        let reply: Reply = serde_json::from_value(params).map_err(|e| bad(e.to_string()))?;
        if reply.conversation.trim().is_empty() || reply.conversation.chars().count() > MAX_CONVERSATION_CHARS {
            return Err(bad(format!("conversation must be 1 to {MAX_CONVERSATION_CHARS} characters")));
        }
        if reply.text.chars().count() > MAX_TEXT_CHARS {
            return Err(bad(format!("text is longer than {MAX_TEXT_CHARS} characters")));
        }
        if reply.images.len() > MAX_IMAGES {
            return Err(bad(format!("at most {MAX_IMAGES} images")));
        }
        if reply.text.trim().is_empty() && reply.images.is_empty() {
            return Err(bad("there is nothing to answer: no text and no image".to_string()));
        }

        let title: String = reply.title.as_deref().filter(|t| !t.trim().is_empty()).unwrap_or(&reply.conversation).chars().take(MAX_TITLE_CHARS).collect();
        let failed = |e: crate::services::error::ErrorService| RpcError::new(INTERNAL, e.message.unwrap_or_else(|| "the agent failed".to_string()));
        let chat = self
            .access
            .chat_store
            .find_or_create_plugin_chat(title, self.plugin_name.clone(), self.plugin_subname.clone(), reply.conversation)
            .await
            .map_err(|e| failed(e.into()))?;
        // Every image is decoded and written again, which drops one the model server could not read (and the rest still count)
        let images = normalize_images(reply.images).await;
        let answer = self.access.agent.reply(chat.id, reply.text, images, Some(ThinkChoice::Enabled(reply.think))).await.map_err(failed)?;
        Ok(json!({"content": answer.content}))
    }
}
