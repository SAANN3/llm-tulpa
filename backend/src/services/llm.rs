//! Talking to a model server. `LlmProvider` (in `provider.rs`) is what the rest of the app calls;
//! each provider converts its own wire format to the shared types in `types.rs`: llama.cpp's own
//! server (`llama_cpp.rs`) and Ollama (`ollama.rs`). `template.rs`, `budget.rs`, `tool_defs.rs` and
//! `prefix_watch.rs` hold what is true of any provider.

mod budget;
mod llama_cpp;
mod ollama;
mod prefix_watch;
mod provider;
mod template;
mod tool_defs;
mod types;

pub use budget::estimated_prompt_tokens;
pub use llama_cpp::LlamaCppProvider;
pub use ollama::{ImportProgress, OllamaService};
pub use provider::{LlmProvider, LlmProviders};
pub use tool_defs::tool_definitions;
pub use types::*;
