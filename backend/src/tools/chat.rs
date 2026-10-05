pub mod get_messages;
pub mod list_messages;
pub mod write_notes;

use super::base::Tool;

/// Every tool in the `chat` domain (function names prefixed `chat.`) — reading the
/// chat this call is happening in: who wrote which message, and the full text of the
/// ones the model asks for by id. That's how a message that scrolled out of the
/// context window comes back into view. `chat.write_notes` is the one that writes: the
/// model's own notes for this chat, which outlive a compaction fold.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(list_messages::ListMessagesTool),
        Box::new(get_messages::GetMessagesTool),
        Box::new(write_notes::WriteNotesTool),
    ]
}
