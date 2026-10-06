//! What the runner and the turn steps share about one run: when it started, when its current model
//! call did, the tokens it has spent, and whether it was told to stop.

use std::sync::{Arc, Mutex};

use sea_orm::prelude::DateTimeUtc;
use tokio_util::sync::CancellationToken;

/// The numbers a page restores "thinking for 1m 30s, N tokens" from after a reload.
#[derive(Clone, Copy)]
pub(super) struct RunSnapshot {
    pub(super) started_at: DateTimeUtc,
    /// When the model call that is in flight started, `None` between calls (a tool is running, say).
    pub(super) call_started_at: Option<DateTimeUtc>,
    /// Tokens the run's finished model calls generated. Calls are not streamed, so this moves by a
    /// whole call's worth at a time.
    pub(super) eval_tokens: u64,
    /// The prompt size the last finished call measured.
    pub(super) prompt_tokens: Option<u64>,
}

#[derive(Clone)]
pub(super) struct RunTracker {
    info: Arc<Mutex<RunSnapshot>>,
    stop: CancellationToken,
}

impl RunTracker {
    pub(super) fn new() -> Self {
        let info = RunSnapshot { started_at: chrono::Utc::now(), call_started_at: None, eval_tokens: 0, prompt_tokens: None };
        Self { info: Arc::new(Mutex::new(info)), stop: CancellationToken::new() }
    }

    pub(super) fn snapshot(&self) -> RunSnapshot {
        *self.info.lock().unwrap()
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
        self.info.lock().unwrap().call_started_at = Some(chrono::Utc::now());
    }

    pub(super) fn call_finished(&self, eval_tokens: Option<u64>, prompt_tokens: Option<u64>) {
        let mut info = self.info.lock().unwrap();
        info.call_started_at = None;
        info.eval_tokens += eval_tokens.unwrap_or(0);
        if prompt_tokens.is_some() {
            info.prompt_tokens = prompt_tokens;
        }
    }

    /// A model call that was dropped (stopped) spent nothing the run can count.
    pub(super) fn call_abandoned(&self) {
        self.info.lock().unwrap().call_started_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(!run.is_stopped());
        run.clone().stop();
        assert!(run.is_stopped());
    }
}
