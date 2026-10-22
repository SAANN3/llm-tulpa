pub mod list_models;
pub mod transcribe;

use super::base::Tool;

/// Every tool in the `stt` domain (function names prefixed `stt.`) — speech to text: turning
/// an audio file into its text with the speech models installed in Settings (Voice).
pub fn collect() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(list_models::ListModelsTool),
        Box::new(transcribe::TranscribeTool),
    ]
}
