pub mod attach_file;

use super::base::Tool;

/// Every tool in the `ui` domain (function names prefixed `ui.`) — things a tool calls
/// to make the frontend show the user something, as opposed to `storage`/`os`/`web`'s
/// tools, which act on the real filesystem/host/network and return data for the model
/// itself. For `main.rs` to register alongside every other domain's `collect()`.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![Box::new(attach_file::AttachFileTool)]
}
