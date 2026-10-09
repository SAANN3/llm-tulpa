//! What the runner and the turn steps share about one run: when it started, when its current model
//! call did, the tokens it has spent, the reply being written, and whether it was told to stop.

use std::sync::{Arc, Mutex};

use sea_orm::prelude::DateTimeUtc;
use tokio_util::sync::CancellationToken;

/// The numbers a page restores "thinking for 1m 30s, N tokens" from after a reload.
#[derive(Clone)]
pub(super) struct RunSnapshot {
    pub(super) started_at: DateTimeUtc,
    /// When the model call that is in flight started, `None` between calls (a tool is running, say).
    pub(super) call_started_at: Option<DateTimeUtc>,
    /// Tokens the run's finished model calls generated. The counts come with a call's complete reply,
    /// so this moves by a whole call's worth at a time.
    pub(super) eval_tokens: u64,
    /// The prompt size the last finished call measured.
    pub(super) prompt_tokens: Option<u64>,
    /// The step the run is on (1 for the first model call), and the user's step limit when one is set.
    pub(super) step: u32,
    pub(super) step_limit: Option<u32>,
    /// The tool call that is running and since when, `None` while no tool runs.
    pub(super) running_tool: Option<String>,
    pub(super) tool_started_at: Option<DateTimeUtc>,
    /// The run is over and its end has been recorded: the chat is only waiting for the claim to be dropped.
    pub(super) ended: bool,
    /// What the model call in flight has written so far, as far as it was sent out in pieces.
    pub(super) reply: ReplySoFar,
}

/// The reply a model call is writing, as sent out so far (`ServerEvent::ReplyPiece`). A page that opens in the middle
/// of it starts from this and then adds the pieces numbered after `seq`, so it shows the whole reply with nothing
/// twice and nothing missing.
#[derive(Clone, Default)]
pub(super) struct ReplySoFar {
    /// Which model call of the run this is (from 1); a reply asked for again is a new one.
    pub(super) number: u32,
    /// How many pieces were sent, so the last one sent is numbered `seq`.
    pub(super) seq: u32,
    pub(super) thinking: String,
    pub(super) text: String,
}

#[derive(Clone)]
pub(super) struct RunTracker {
    info: Arc<Mutex<RunSnapshot>>,
    stop: CancellationToken,
}

impl RunTracker {
    pub(super) fn new() -> Self {
        let info = RunSnapshot {
            started_at: chrono::Utc::now(),
            call_started_at: None,
            eval_tokens: 0,
            prompt_tokens: None,
            step: 0,
            step_limit: None,
            running_tool: None,
            tool_started_at: None,
            ended: false,
            reply: ReplySoFar::default(),
        };
        Self { info: Arc::new(Mutex::new(info)), stop: CancellationToken::new() }
    }

    pub(super) fn snapshot(&self) -> RunSnapshot {
        self.info.lock().unwrap().clone()
    }

    pub(super) fn set_step(&self, step: u32) {
        self.info.lock().unwrap().step = step;
    }

    pub(super) fn set_step_limit(&self, limit: Option<u32>) {
        self.info.lock().unwrap().step_limit = limit;
    }

    pub(super) fn tool_started(&self, tool_name: &str) {
        let mut info = self.info.lock().unwrap();
        info.running_tool = Some(tool_name.to_string());
        info.tool_started_at = Some(chrono::Utc::now());
    }

    pub(super) fn mark_ended(&self) {
        self.info.lock().unwrap().ended = true;
    }

    pub(super) fn tool_finished(&self) {
        let mut info = self.info.lock().unwrap();
        info.running_tool = None;
        info.tool_started_at = None;
    }

    /// Tells the run to stop: the model call in flight is dropped (which closes its request, so the
    /// server stops generating) and the loop ends before its next step.
    pub(super) fn stop(&self) {
        self.stop.cancel();
    }

    pub(super) fn is_stopped(&self) -> bool {
        self.stop.is_cancelled()
    }

    /// Resolves once `stop` was called.
    pub(super) async fn stopped(&self) {
        self.stop.cancelled().await
    }

    pub(super) fn call_started(&self) {
        let mut info = self.info.lock().unwrap();
        info.call_started_at = Some(chrono::Utc::now());
        info.reply = ReplySoFar { number: info.reply.number + 1, ..ReplySoFar::default() };
    }

    /// Adds a piece of the reply being written and numbers it: what the piece's event carries.
    pub(super) fn reply_piece(&self, thinking: &str, text: &str) -> (u32, u32) {
        let mut info = self.info.lock().unwrap();
        let reply = &mut info.reply;
        reply.thinking.push_str(thinking);
        reply.text.push_str(text);
        reply.seq += 1;
        (reply.number, reply.seq)
    }

    pub(super) fn call_finished(&self, eval_tokens: Option<u64>, prompt_tokens: Option<u64>) {
        let mut info = self.info.lock().unwrap();
        info.call_started_at = None;
        info.reply = ReplySoFar { number: info.reply.number, ..ReplySoFar::default() };
        info.eval_tokens += eval_tokens.unwrap_or(0);
        if prompt_tokens.is_some() {
            info.prompt_tokens = prompt_tokens;
        }
    }

    /// A model call that was dropped (stopped) spent nothing the run can count.
    pub(super) fn call_abandoned(&self) {
        let mut info = self.info.lock().unwrap();
        info.call_started_at = None;
        info.reply = ReplySoFar { number: info.reply.number, ..ReplySoFar::default() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reply_so_far_is_numbered_per_call_and_cleared_when_it_ends() {
        let run = RunTracker::new();
        run.call_started();
        assert_eq!(run.reply_piece("hm", ""), (1, 1));
        assert_eq!(run.reply_piece(" ok", "Hi"), (1, 2));
        let reply = run.snapshot().reply;
        assert_eq!((reply.thinking.as_str(), reply.text.as_str()), ("hm ok", "Hi"));
        run.call_finished(None, None);
        assert_eq!(run.snapshot().reply.seq, 0);
        // The next call (a next step, or the same reply asked for again) starts its own numbering
        run.call_started();
        assert_eq!(run.reply_piece("", "x"), (2, 1));
        run.call_abandoned();
        assert!(run.snapshot().reply.text.is_empty());
    }

    #[test]
    fn tokens_add_up_per_finished_call_and_a_stop_is_seen() {
        let run = RunTracker::new();
        run.call_started();
        assert!(run.snapshot().call_started_at.is_some());
        run.call_finished(Some(300), Some(12_000));
        run.call_started();
        run.call_finished(Some(50), None);
        let snapshot = run.snapshot();
        assert_eq!(snapshot.eval_tokens, 350);
        assert_eq!(snapshot.prompt_tokens, Some(12_000));
        assert!(snapshot.call_started_at.is_none());

        run.set_step(3);
        run.tool_started("os.execute_command");
        assert_eq!(run.snapshot().running_tool.as_deref(), Some("os.execute_command"));
        assert_eq!(run.snapshot().step, 3);
        run.tool_finished();
        assert!(run.snapshot().running_tool.is_none() && run.snapshot().tool_started_at.is_none());

        assert!(!run.is_stopped());
        run.clone().stop();
        assert!(run.is_stopped());
    }
}
