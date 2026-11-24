//! One-shot model calls: a fixed instruction, and the data to work on as a user message of its own.
//! No chat history or tools. `ask` keeps thinking off for instruction/data helpers.
//!
//! The data is untrusted text (a conversation excerpt, a note someone typed), not instructions, so it
//! goes in a separate `user` message instead of being interpolated into the instruction: the model
//! then has the same role-based signal for "this is data to work on, not a request to act on" that a
//! real conversation gets, instead of a delimiter it has to be talked into respecting.
//!
//! Used by the agent (the compaction summary and the key facts) and by `PromptFacade` (names).

use crate::services::{
    error::ErrorService,
    llm::{CallParams, ChatMessage, ChatResponse, LlmProviders, OnPiece, ThinkChoice},
};

#[derive(Clone)]
pub struct OneShot {
    providers: LlmProviders,
}

impl OneShot {
    pub fn new(providers: LlmProviders) -> Self {
        Self { providers }
    }

    /// One reply to `system` with `data` as the user message, on the given model. With `on_piece` the reply is also
    /// handed over as it is written (see `LlmProvider::chat`); what is returned is the same either way.
    pub async fn ask(
        &self,
        provider: &str,
        model: &str,
        system: String,
        data: ChatMessage,
        on_piece: Option<OnPiece<'_>>,
    ) -> Result<ChatResponse, ErrorService> {
        self.complete(provider, model, vec![ChatMessage::system(system), data],
            Some(ThinkChoice::Enabled(false)), &CallParams::default(), on_piece).await
    }

    /// A single completion with caller-supplied messages and model parameters, without tools.
    /// The caller owns any claim that must span model preparation and the call.
    pub async fn complete(
        &self,
        provider: &str,
        model: &str,
        messages: Vec<ChatMessage>,
        think: Option<ThinkChoice>,
        params: &CallParams,
        on_piece: Option<OnPiece<'_>>,
    ) -> Result<ChatResponse, ErrorService> {
        Ok(self.providers.get(provider)?.chat(
            messages, None, &[], think, model, None, params, on_piece,
        ).await?)
    }

    /// Asks until `check` accepts the reply and returns its text: one attempt per entry of `closings`,
    /// in order, each with the user message `data_for` builds from that closing (a later closing is
    /// the sharper one). `check` gets the reply and the model's own tool-call tags, and names what is
    /// wrong with it, or `None` when it is fine. When every attempt is refused the error carries the
    /// last problem, and the caller decides what to keep. `on_piece` hears every attempt.
    #[allow(clippy::too_many_arguments)]
    pub async fn ask_checked(
        &self,
        provider: &str,
        model: &str,
        system: String,
        data_for: impl Fn(&str) -> ChatMessage,
        closings: &[&str],
        check: impl Fn(&ChatMessage, &[String]) -> Option<&'static str>,
        on_piece: Option<OnPiece<'_>>,
    ) -> Result<String, ErrorService> {
        let markers = self.providers.get(provider)?.tool_call_markers(model).await;
        let mut last_problem = "";
        for closing in closings {
            let response = self.ask(provider, model, system.clone(), data_for(closing), on_piece).await?;
            match check(&response.message, &markers) {
                None => return Ok(response.message.content.trim().to_string()),
                Some(problem) => {
                    tracing::warn!(problem, "one-shot reply rejected");
                    last_problem = problem;
                }
            }
        }
        Err(ErrorService::internal(format!("the model gave no usable reply ({last_problem})")))
    }
}
