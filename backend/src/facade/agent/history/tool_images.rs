//! A picture a tool handed over (`llm.read_image`), as the model server takes it.
//!
//! The picture is stored on the tool's own message, but a tool message can't carry an image to the
//! model server (its content is text, and chat templates render nothing else there). So the request
//! gets the pictures as a user message of their own right after the run of tool results they came
//! with, which keeps every tool call next to its result. That message exists only in the request: it is
//! built again from the stored tool messages each time, so the chat shows the picture under its tool
//! call and nothing the user did not write.

use super::super::prompts;
use crate::services::llm::ChatMessage;

/// `messages` with each run of tool results that carried pictures followed by one user message holding
/// them, in the order the results came.
pub(super) fn after_tool_results(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(messages.len());
    let mut images: Vec<String> = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    for mut message in messages {
        if message.role != "tool" && !images.is_empty() {
            out.push(picture_message(std::mem::take(&mut images), std::mem::take(&mut tools)));
        }
        if message.role == "tool" {
            if let Some(attached) = message.images.take().filter(|attached| !attached.is_empty()) {
                tools.push(message.tool_name.clone().unwrap_or_else(|| "a tool".to_string()));
                images.extend(attached);
            }
        }
        out.push(message);
    }
    if !images.is_empty() {
        out.push(picture_message(images, tools));
    }
    out
}

fn picture_message(images: Vec<String>, tools: Vec<String>) -> ChatMessage {
    ChatMessage::user_with_images(prompts::tool_images_note(&tools), images)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str, images: Option<Vec<String>>) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_name: (role == "tool").then(|| "llm.read_image".to_string()),
            thinking: None,
            images,
        }
    }

    #[test]
    fn a_picture_follows_the_run_of_tool_results_it_came_with_and_leaves_the_tool_message_text_only() {
        let out = after_tool_results(vec![
            message("user", "look at it", None),
            message("assistant", "", None),
            message("tool", "one", Some(vec!["AAAA".to_string()])),
            message("tool", "two", None),
            message("assistant", "done", None),
        ]);
        let roles: Vec<&str> = out.iter().map(|m| m.role.as_str()).collect();
        assert_eq!(roles, ["user", "assistant", "tool", "tool", "user", "assistant"]);
        assert!(out[2].images.is_none() && out[3].images.is_none());
        assert_eq!(out[4].images.as_deref(), Some(&["AAAA".to_string()][..]));
        assert!(out[4].content.contains("llm.read_image"), "{}", out[4].content);
    }

    #[test]
    fn a_run_that_ends_the_history_gets_its_picture_last_and_two_pictures_share_one_message() {
        let out = after_tool_results(vec![
            message("assistant", "", None),
            message("tool", "a", Some(vec!["AAAA".to_string()])),
            message("tool", "b", Some(vec!["BBBB".to_string()])),
        ]);
        assert_eq!(out.len(), 4);
        assert_eq!(out[3].role, "user");
        assert_eq!(out[3].images.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn a_history_without_pictures_comes_out_unchanged() {
        let out = after_tool_results(vec![message("user", "hi", None), message("tool", "x", None)]);
        assert_eq!(out.len(), 2);
    }
}
