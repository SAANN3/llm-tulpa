pub mod read_image;

use super::base::Tool;

/// Every tool in the `llm` domain (function names prefixed `llm.`) — tools that make
/// their own separate call to the model itself, as opposed to every other domain's
/// tools, which act on the filesystem/host/network/UI and just return data. For
/// `main.rs` to register alongside every other domain's `collect()`.
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![Box::new(read_image::ReadImageTool)]
}
