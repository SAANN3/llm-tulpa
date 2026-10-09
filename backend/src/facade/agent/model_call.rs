//! Everything about calling the model for a chat that is not the prompt itself: which provider and
//! launch profile a chat runs on, the call parameters and context window that follow from them,
//! the claim a turn keeps on the model server, and the policy for a reply that can't be used.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::facade::launch::LaunchFacade;
use crate::services::chat_store::{Chat, MessageTimings};
use crate::services::error::ErrorService;
use crate::services::llama_runtime::CallGuard;
use crate::services::llm::{CallParams, ChatResponse, LaunchRequest, LlmProvider, LlmProviders};
use crate::services::preset_store::PresetStore;

/// How long a turn's claim on the model server outlasts its last model call.
const TURN_HOLD_TTL: Duration = Duration::from_secs(10 * 60);

/// What a chat ran on when its turn started: a model change made while the turn goes on waits for
/// the next prompt.
#[derive(Clone)]
struct TurnBinding {
    model_id: i64,
    provider: String,
    model: String,
    launch_profile_id: Option<i64>,
}

struct TurnHold {
    _guard: CallGuard,
    generation: u64,
    binding: TurnBinding,
}

#[derive(Clone)]
pub(super) struct ModelCall {
    providers: LlmProviders,
    presets: Arc<PresetStore>,
    launch: Arc<LaunchFacade>,
    /// The configured context window: what a chat with no launch profile runs under.
    context_length: u64,
    /// What keeps the model server on a chat's launch profile from the turn's first model call until
    /// its final reply, across the tool runs and permission prompts between model calls — see `hold`.
    turn_holds: Arc<Mutex<HashMap<i64, TurnHold>>>,
}

impl ModelCall {
    pub(super) fn new(
        providers: LlmProviders,
        presets: Arc<PresetStore>,
        launch: Arc<LaunchFacade>,
        context_length: u64,
    ) -> Self {
        Self { providers, presets, launch, context_length, turn_holds: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// The provider `chat`'s model is served by.
    pub(super) fn provider(&self, chat: &Chat) -> Result<Arc<dyn LlmProvider>, ErrorService> {
        self.providers.get(&chat.provider)
    }

    /// What a call for this chat may override about how its model runs: the context window of the
    /// launch profile it runs on (when the profile fixes one), and the sampling the chat's user has
    /// chosen for the model. A chat on a model with no profile, and a user with no chosen preset,
    /// override nothing and get the server's own behavior.
    pub(super) async fn params(&self, chat: &Chat) -> Result<CallParams, ErrorService> {
        let launch = self.launch.request_for(chat.user_id, chat.model_id, chat.launch_profile_id).await?;
        let context_length = launch.as_ref().and_then(|l| l.profile.context_length).map(|c| c as u64);
        let sampling = self.presets.effective_sampling(chat.user_id, chat.model_id).await?;
        Ok(CallParams { launch, context_length, sampling })
    }

    /// The context window `chat`'s model runs under.
    pub(super) async fn context_of(&self, chat: &Chat) -> Result<u64, ErrorService> {
        Ok(self.launch.contexts(self.context_length).await?.for_profile(chat.launch_profile_id))
    }

    /// Switching the model of a chat in the middle of its turn takes effect with the next prompt:
    /// while the turn holds the server, `chat` is changed back to what the turn started on.
    pub(super) fn bind(&self, chat: &mut Chat) {
        let held = self.turn_holds.lock().unwrap().get(&chat.id).map(|h| h.binding.clone());
        if let Some(held) = held {
            chat.model_id = held.model_id;
            chat.provider = held.provider;
            chat.model = held.model;
            chat.launch_profile_id = held.launch_profile_id;
        }
    }

    /// Claims the model server for `chat`'s turn on its launch profile, and keeps the claim until
    /// the turn ends (`release`) or `TURN_HOLD_TTL` passes without another model call, which is how a
    /// turn left at a permission prompt that nobody answers lets go. While held, another user who
    /// needs a different profile is told to wait instead of reloading under the turn and discarding
    /// its cached prompt.
    /// Returns how long the claim waited for the model to load, when this was the call that loaded it.
    pub(super) async fn hold(&self, chat: &Chat, provider: &dyn LlmProvider, launch: Option<&LaunchRequest>) -> Result<Option<i64>, ErrorService> {
        let chat_id = chat.id;
        let guard = provider.acquire(launch).await?;
        let loaded_ms = guard.loaded_ms;
        let generation = {
            let mut holds = self.turn_holds.lock().unwrap();
            let generation = holds.get(&chat_id).map_or(0, |h| h.generation + 1);
            let binding = holds.get(&chat_id).map(|h| h.binding.clone()).unwrap_or_else(|| TurnBinding {
                model_id: chat.model_id,
                provider: chat.provider.clone(),
                model: chat.model.clone(),
                launch_profile_id: chat.launch_profile_id,
            });
            // The old claim (if any) is dropped here, after the new one is in place
            holds.insert(chat_id, TurnHold { _guard: guard, generation, binding });
            generation
        };
        let holds = self.turn_holds.clone();
        tokio::spawn(async move {
            tokio::time::sleep(TURN_HOLD_TTL).await;
            let mut holds = holds.lock().unwrap();
            if holds.get(&chat_id).is_some_and(|h| h.generation == generation) {
                holds.remove(&chat_id);
            }
        });
        Ok(loaded_ms)
    }

    /// Lets go of the claim `hold` made: the turn is over.
    pub(super) fn release(&self, chat_id: i64) {
        self.turn_holds.lock().unwrap().remove(&chat_id);
    }

    /// Whether a model reply is unusable — see `ReplyProblem` — and so should be asked for
    /// again rather than stored. A reply that carries a real tool call is always usable,
    /// and so is one that was cut off by the token limit: asking again would just run
    /// into the same wall (`num_predict` already leaves all the room there is).
    ///
    /// Tool-call-as-text is recognized by the model's own wrapper tags (`<tool_call>` /
    /// `</tool_call>` for Qwen, whatever another model's template says — see
    /// `LlmProvider::tool_call_markers`) turning up in its reasoning or answer with no
    /// call actually parsed. Emptiness alone deliberately doesn't count as that: it can be
    /// a legitimate reply, so it's its own, more cautious, problem.
    pub(super) async fn unusable_reply(provider: &dyn LlmProvider, response: &ChatResponse, model: &str) -> Option<ReplyProblem> {
        let message = &response.message;
        if message.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()) {
            return None;
        }
        if response.done_reason.as_deref() == Some("length") {
            if message.content.trim().is_empty() {
                return Some(ReplyProblem::CutOffInThinking);
            }
            return None;
        }

        // The wrapper tags always contain "tool_call"/"function_call" (that's how
        // they're found), so text without either can't contain one — which keeps the
        // template lookup off the path of every ordinary reply.
        let texts = [message.thinking.as_deref().unwrap_or_default(), message.content.as_str()];
        if texts.iter().any(|text| text.contains("tool_call") || text.contains("function_call")) {
            let markers = provider.tool_call_markers(model).await;
            if texts.iter().any(|text| markers.iter().any(|marker| text.contains(marker.as_str()))) {
                return Some(ReplyProblem::ToolCallAsText);
            }
        }

        message.content.trim().is_empty().then_some(ReplyProblem::Empty)
    }

    /// What Ollama reported about the time the call behind `response` took, for storing with its message
    pub(super) fn timings_of(response: &ChatResponse) -> MessageTimings {
        MessageTimings {
            eval_ms: response.eval_duration_ms(),
            prompt_eval_ms: response.prompt_eval_duration_ms(),
            load_ms: response.load_duration_ms(),
            prompt_processed: response.prompt_processed_tokens(),
        }
    }
}

/// Why a model reply was thrown away and asked for again instead of being stored.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplyProblem {
    /// The model wrote a tool call out as text (in its reasoning or its answer) instead
    /// of actually calling the tool, so nothing ran and the turn would just end there.
    ToolCallAsText,
    /// No answer, no tool call, and it wasn't cut off — nothing to show or act on.
    Empty,
    /// The model was cut off by the token limit strictly while still in its reasoning trace
    /// (empty content and no tool calls), leaving an incomplete and stalled reply.
    CutOffInThinking,
}

impl ReplyProblem {
    pub(super) fn describe(self) -> &'static str {
        match self {
            ReplyProblem::ToolCallAsText => "tool call written out as text instead of being called",
            ReplyProblem::Empty => "empty reply (no answer, no tool call)",
            ReplyProblem::CutOffInThinking => "reply cut off by token limit while still in thinking",
        }
    }
}

/// How many times a turn regenerates one reply before keeping whatever it got. A
/// malformed tool call is a sampling accident — asking again almost always fixes it —
/// so it gets two more tries. An empty reply can just as well be the right answer (the
/// user asked it to say nothing), which asking again will only repeat, so it gets one:
/// enough to catch a stall, cheap enough when it wasn't one. Cut off in thinking gets one retry
/// after emergency compaction so the model can reason to completion with fresh headroom.
#[derive(Default)]
pub(super) struct Regenerations {
    tool_call_as_text: u8,
    empty: u8,
    cut_off_in_thinking: u8,
}

impl Regenerations {
    pub(super) fn allow(&mut self, problem: ReplyProblem) -> bool {
        let (used, max) = match problem {
            ReplyProblem::ToolCallAsText => (&mut self.tool_call_as_text, 2),
            ReplyProblem::Empty => (&mut self.empty, 1),
            ReplyProblem::CutOffInThinking => (&mut self.cut_off_in_thinking, 1),
        };
        if *used >= max {
            return false;
        }
        *used += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_problem_is_regenerated_up_to_its_own_limit() {
        let mut regenerations = Regenerations::default();
        assert!(regenerations.allow(ReplyProblem::ToolCallAsText));
        assert!(regenerations.allow(ReplyProblem::ToolCallAsText));
        assert!(!regenerations.allow(ReplyProblem::ToolCallAsText));
        assert!(regenerations.allow(ReplyProblem::Empty));
        assert!(!regenerations.allow(ReplyProblem::Empty));
        assert!(regenerations.allow(ReplyProblem::CutOffInThinking));
        assert!(!regenerations.allow(ReplyProblem::CutOffInThinking));
    }
}
