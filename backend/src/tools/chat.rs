pub mod get_messages;
pub mod list_messages;

use super::base::Tool;

/// Every tool in the `chat` domain (function names prefixed `chat.`) — reading the
/// chat this call is happening in: who wrote which message, and the full text of the
/// ones the model asks for by id. That's how a message that scrolled out of the
/// context window comes back into view.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(list_messages::ListMessagesTool),
        Box::new(get_messages::GetMessagesTool),
    ]
}
