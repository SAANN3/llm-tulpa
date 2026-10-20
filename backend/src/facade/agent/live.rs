//! What a run tells a page about its work, beyond the text of the reply. Two things are reported, both as whole
//! snapshots that the `RunTracker` keeps as well as sends, so a page that opens in the middle of the work starts from
//! the same picture a page that watched it from the start has:
//!
//! - the model call in flight (`Live::call`): how far the server has got reading the prompt, how fast it generates
//!   (`ServerEvent::CallProgress`);
//! - a list of steps (`Live::steps`): any work made of steps one after another, each with a state, a few words of detail,
//!   how far it has got and, when it makes one, its model call (`ServerEvent::RunSteps`). Nothing in a step says what
//!   the work is: a fold of the chat, a plan the model carries out and a batch of files are the same to this module and
//!   to the page. What a kind of work adds is a title and a `StepDef` per step, next to the code that does it. Lists
//!   are independent: work that starts while other work goes on (a fold in the middle of a plan) is a list of its own,
//!   and ending one leaves the others as they were.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::run_tracker::RunTracker;
use crate::services::event_bus::{CallKind, CallStatus, EventBus, RunStep, ServerEvent, StepList, StepProgress, StepState};
use crate::services::llm::ReplyPiece;

/// How often the speed of a call in flight goes out at most: it changes with every token, and a page showing
/// it needs a few updates a second, not hundreds. The prompt's progress is not held back: the server reports it
/// once per batch, a handful of times per call.
const SPEED_INTERVAL: Duration = Duration::from_millis(200);
/// How often a step's progress goes out at most, for work that counts a thing at a time: the caller says what it
/// counted as often as it likes. The last count, the one that completes the work, is never held back.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// A kind of step: what it is called in its list and for the page. Fixed steps are constants (`StepDef::fixed`);
/// the steps of work that is only known when it starts, a plan the model wrote, are made from the text it gave.
#[derive(Clone, Debug)]
pub(super) struct StepDef {
    id: Cow<'static, str>,
    label: Cow<'static, str>,
}

impl StepDef {
    pub(super) const fn fixed(id: &'static str, label: &'static str) -> Self {
        Self { id: Cow::Borrowed(id), label: Cow::Borrowed(label) }
    }

    /// Only used by work whose steps are made at run time: nothing is yet
    #[allow(dead_code)]
    pub(super) fn owned(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self { id: Cow::Owned(id.into()), label: Cow::Owned(label.into()) }
    }

    fn to_step(&self) -> RunStep {
        RunStep { id: self.id.to_string(), label: self.label.to_string(), state: StepState::Pending, detail: None, progress: None, call: None }
    }
}

/// The run a report belongs to
#[derive(Clone)]
pub(super) struct Live {
    chat_id: i64,
    events: Arc<EventBus>,
    run: RunTracker,
}

impl Live {
    pub(super) fn new(chat_id: i64, events: Arc<EventBus>, run: RunTracker) -> Self {
        Self { chat_id, events, run }
    }

    /// Resolves once the user has stopped the run these reports belong to
    pub(super) async fn stopped(&self) {
        self.run.stopped().await
    }

    /// Starts reporting a model call of `kind`; the report ends with the value dropped, however the call ends
    /// (a reply, an error, a stop that drops the call mid-flight).
    pub(super) fn call(&self, kind: CallKind) -> CallReport {
        self.call_for(kind, None)
    }

    fn call_for(&self, kind: CallKind, step: Option<StepSink>) -> CallReport {
        let report = CallReport {
            live: self.clone(),
            inner: Mutex::new((CallStatus { kind, prompt: None, generation: None }, Instant::now())),
            step,
        };
        {
            let inner = report.inner.lock().unwrap();
            report.send(inner.0);
        }
        report
    }

    /// Starts reporting work called `title` made of `defs`, in order, all waiting; ends with the value dropped. It is a
    /// list of its own: work that starts while other work goes on (a fold in the middle of a plan) doesn't touch it.
    pub(super) fn steps(&self, title: &str, defs: &[StepDef]) -> StepsReport {
        let list = StepList { id: self.run.new_list_id(), title: title.to_string(), steps: defs.iter().map(StepDef::to_step).collect() };
        let report = StepsReport { live: self.clone(), list_id: list.id, list: Arc::new(Mutex::new(list)), progress_sent: Mutex::new(HashMap::new()) };
        report.send(&report.list.lock().unwrap());
        report
    }

    // Always called with the list locked, so reports leave in the order they were made
    fn send_list(&self, list: &StepList) {
        let lists = self.run.set_list(list.clone());
        self.events.publish(ServerEvent::RunSteps { chat_id: self.chat_id, lists });
    }
}

/// Where a step's model call reports to: the step it belongs to
struct StepSink {
    list: Arc<Mutex<StepList>>,
    id: String,
}

/// The report of one model call in flight
pub(super) struct CallReport {
    live: Live,
    /// What was last reported, and when its speed last went out
    inner: Mutex<(CallStatus, Instant)>,
    /// The step this call is part of, when it is: the call is reported through the step's snapshot and what it came to
    /// stays in it
    step: Option<StepSink>,
}

impl CallReport {
    /// Takes in a piece of the call's reply; only the prompt's progress and the speed matter here (the text is
    /// `PieceBatcher`'s)
    pub(super) fn piece(&self, piece: &ReplyPiece) {
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match piece {
            ReplyPiece::Prompt(progress) => {
                // A call is asked again (a summary that was refused): its prompt is read from the start, and the
                // speed shown is not the last attempt's
                inner.0.generation = None;
                inner.0.prompt = Some(*progress);
                self.send(inner.0);
            }
            ReplyPiece::Generation(generation) => {
                // The first figure goes out at once: it is what tells a page the prompt is read and the reply began
                let first = inner.0.generation.is_none();
                inner.0.generation = Some(*generation);
                if first || inner.1.elapsed() >= SPEED_INTERVAL {
                    inner.1 = Instant::now();
                    self.send(inner.0);
                }
            }
            ReplyPiece::Thinking(_) | ReplyPiece::Text(_) => {}
        }
    }

    // Always called with `inner` locked, so reports leave in the order they were made
    fn send(&self, call: CallStatus) {
        match &self.step {
            Some(sink) => self.live.edit_step(&sink.list, &sink.id, |step| step.call = Some(call)),
            None => {
                self.live.run.set_call(Some(call));
                self.live.events.publish(ServerEvent::CallProgress { chat_id: self.live.chat_id, call: Some(call) });
            }
        }
    }
}

impl Drop for CallReport {
    fn drop(&mut self) {
        let inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match &self.step {
            // What the call came to, including the last figures the interval held back
            Some(sink) => self.live.edit_step(&sink.list, &sink.id, |step| step.call = Some(inner.0)),
            None => {
                self.live.run.set_call(None);
                self.live.events.publish(ServerEvent::CallProgress { chat_id: self.live.chat_id, call: None });
            }
        }
    }
}

impl Live {
    /// Changes one step of `list` and reports the whole list
    fn edit_step(&self, list: &Mutex<StepList>, id: &str, change: impl FnOnce(&mut RunStep)) {
        let mut list = list.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(step) = list.steps.iter_mut().find(|step| step.id == id) {
            change(step);
        }
        self.send_list(&list);
    }
}

/// The report of work made of steps: which are waiting, running, done, failed or not needed, and how each is going
pub(super) struct StepsReport {
    live: Live,
    list_id: u64,
    list: Arc<Mutex<StepList>>,
    /// When each step's progress last went out
    progress_sent: Mutex<HashMap<String, Instant>>,
}

impl StepsReport {
    /// Sets how `def`'s step stands; `detail` says what it came to, or why it was skipped or failed
    pub(super) fn set(&self, def: &StepDef, state: StepState, detail: Option<&str>) {
        self.live.edit_step(&self.list, &def.id, |step| {
            step.state = state;
            step.detail = detail.map(str::to_string);
        });
    }

    /// Starts `def`'s step, for work that makes no model call
    pub(super) fn start(&self, def: &StepDef) {
        self.set(def, StepState::Running, None);
    }

    /// Starts `def`'s step and the report of the model call of `kind` it makes
    pub(super) fn call(&self, def: &StepDef, kind: CallKind) -> CallReport {
        self.start(def);
        self.live.call_for(kind, Some(StepSink { list: self.list.clone(), id: def.id.to_string() }))
    }

    /// Says how far `def`'s step has got. It may be called as often as the work counts: reports go out a few times a
    /// second, and the one that completes the work (`done` reaching `total`) always goes out. No work counts a thing at
    /// a time yet (a fold's steps are model calls, whose progress is the call's own), so nothing calls this outside the
    /// tests: it is what a step that does, such as one file of many, reports through.
    #[allow(dead_code)]
    pub(super) fn progress(&self, def: &StepDef, done: u64, total: Option<u64>) {
        let last = total.is_some_and(|total| done >= total);
        {
            let mut sent = self.progress_sent.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if !last && sent.get(&*def.id).is_some_and(|at| at.elapsed() < PROGRESS_INTERVAL) {
                return;
            }
            sent.insert(def.id.to_string(), Instant::now());
        }
        self.live.edit_step(&self.list, &def.id, |step| step.progress = Some(StepProgress { done, total }));
    }

    /// Adds a step at the end of the list, waiting; for work whose steps are found as it goes. Does nothing when the list
    /// has a step with that id.
    #[allow(dead_code)]
    pub(super) fn add(&self, def: &StepDef) {
        let mut list = self.list.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if list.steps.iter().all(|step| step.id != *def.id) {
            list.steps.push(def.to_step());
        }
        self.send(&list);
    }

    /// The steps not reached are not needed after all (the work stopped early, or there was nothing to do)
    pub(super) fn skip_waiting(&self, detail: &str) {
        let mut list = self.list.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for step in list.steps.iter_mut().filter(|step| step.state == StepState::Pending) {
            step.state = StepState::Skipped;
            step.detail = Some(detail.to_string());
        }
        self.send(&list);
    }

    // Always called with `list` locked, so reports leave in the order they were made
    fn send(&self, list: &StepList) {
        self.live.send_list(list);
    }
}

impl Drop for StepsReport {
    fn drop(&mut self) {
        // Only this list goes: any other that is still going on stays
        let _list = self.list.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let lists = self.live.run.remove_list(self.list_id);
        self.live.events.publish(ServerEvent::RunSteps { chat_id: self.live.chat_id, lists });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::llm::{Generation, PromptProgress};

    const FIRST: StepDef = StepDef::fixed("first", "The first");
    const SECOND: StepDef = StepDef::fixed("second", "The second");
    const THIRD: StepDef = StepDef::fixed("third", "The third");

    fn live() -> (Live, tokio::sync::broadcast::Receiver<ServerEvent>, RunTracker) {
        let events = Arc::new(EventBus::new());
        let receiver = events.subscribe();
        let run = RunTracker::new();
        (Live::new(7, events, run.clone()), receiver, run)
    }

    fn calls(receiver: &mut tokio::sync::broadcast::Receiver<ServerEvent>) -> Vec<Option<CallStatus>> {
        let mut out = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            if let ServerEvent::CallProgress { call, .. } = event {
                out.push(call);
            }
        }
        out
    }

    #[test]
    fn a_call_reports_its_prompt_then_its_speed_and_ends_with_nothing() {
        let (live, mut receiver, run) = live();
        let report = live.call(CallKind::Reply);
        report.piece(&ReplyPiece::Prompt(PromptProgress { total: 100, cached: 20, processed: 60, elapsed_ms: 500 }));
        report.piece(&ReplyPiece::Generation(Generation { tokens: 1, tokens_per_second: 9.0 }));
        // Within the interval of the one that went out: held back
        report.piece(&ReplyPiece::Generation(Generation { tokens: 2, tokens_per_second: 10.0 }));
        assert_eq!(run.snapshot().call.map(|call| call.generation.map(|g| g.tokens)), Some(Some(1)));
        drop(report);

        let seen = calls(&mut receiver);
        // Started empty, the prompt, the first figure, then the end
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[0], Some(CallStatus { kind: CallKind::Reply, prompt: None, generation: None }));
        assert_eq!(seen[1].and_then(|call| call.prompt).map(|p| p.processed), Some(60));
        assert_eq!(seen[2].and_then(|call| call.generation).map(|g| g.tokens), Some(1));
        assert_eq!(seen[3], None);
        assert!(run.snapshot().call.is_none());
    }

    #[test]
    fn steps_are_reported_whatever_they_are_and_end_with_nothing() {
        let (live, mut receiver, run) = live();
        let report = live.steps("Some work", &[FIRST, SECOND, THIRD]);
        // A step that makes a model call keeps what the call came to; one that doesn't has no call
        drop(report.call(&FIRST, CallKind::Notes));
        report.set(&FIRST, StepState::Done, Some("all done"));
        report.start(&SECOND);
        report.skip_waiting("not needed");
        let lists = run.snapshot().steps;
        assert_eq!(lists.len(), 1);
        let list = &lists[0];
        assert_eq!(list.title, "Some work");
        assert_eq!(list.steps.iter().map(|step| step.state).collect::<Vec<_>>(), vec![StepState::Done, StepState::Running, StepState::Skipped]);
        assert_eq!(list.steps[0].detail.as_deref(), Some("all done"));
        assert_eq!(list.steps[0].call.map(|call| call.kind), Some(CallKind::Notes));
        assert_eq!(list.steps[1].call, None);
        assert_eq!(list.steps[2].detail.as_deref(), Some("not needed"));
        drop(report);
        assert!(run.snapshot().steps.is_empty());

        let mut last = None;
        while let Ok(event) = receiver.try_recv() {
            if let ServerEvent::RunSteps { lists, .. } = event {
                last = Some(lists);
            }
        }
        assert_eq!(last, Some(vec![]));
        // A step's call is reported with its step, not as the run's own call
        assert!(calls(&mut receiver).is_empty());
    }

    #[test]
    fn work_started_in_the_middle_of_other_work_is_a_list_of_its_own() {
        let (live, mut receiver, run) = live();
        let plan = live.steps("The plan", &[FIRST, SECOND]);
        plan.start(&FIRST);
        // A fold begins while the plan goes on, and ends first
        let fold = live.steps("The fold", &[THIRD]);
        fold.start(&THIRD);
        plan.set(&FIRST, StepState::Done, Some("first done"));
        let titles = |run: &RunTracker| run.snapshot().steps.iter().map(|list| list.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(&run), vec!["The plan", "The fold"]);
        // Each report changes its own list only
        assert_eq!(run.snapshot().steps[0].steps[0].state, StepState::Done);
        assert_eq!(run.snapshot().steps[1].steps[0].state, StepState::Running);
        drop(fold);
        // The plan is still there, as it was, and what a page is told says so
        assert_eq!(titles(&run), vec!["The plan"]);
        assert_eq!(run.snapshot().steps[0].steps[0].detail.as_deref(), Some("first done"));
        let mut last = None;
        while let Ok(event) = receiver.try_recv() {
            if let ServerEvent::RunSteps { lists, .. } = event {
                last = Some(lists);
            }
        }
        assert_eq!(last.map(|lists| lists.len()), Some(1));
        drop(plan);
        assert!(run.snapshot().steps.is_empty());
    }

    #[test]
    fn steps_made_at_run_time_can_be_added_as_the_work_goes() {
        let (live, _receiver, run) = live();
        let report = live.steps("Found as it goes", &[StepDef::owned("a", "First thing found")]);
        report.add(&StepDef::owned("b", "Second thing found"));
        // The same id again changes nothing
        report.add(&StepDef::owned("b", "Another label"));
        let labels = run.snapshot().steps[0].steps.iter().map(|step| step.label.clone()).collect::<Vec<_>>();
        assert_eq!(labels, vec!["First thing found", "Second thing found"]);
    }

    #[test]
    fn a_steps_progress_is_held_back_between_reports_except_the_last() {
        let (live, _receiver, run) = live();
        let report = live.steps("Counting", &[FIRST]);
        report.start(&FIRST);
        report.progress(&FIRST, 1, Some(10));
        report.progress(&FIRST, 2, Some(10));
        let progress = |run: &RunTracker| run.snapshot().steps[0].steps[0].progress;
        assert_eq!(progress(&run), Some(StepProgress { done: 1, total: Some(10) }));
        report.progress(&FIRST, 10, Some(10));
        assert_eq!(progress(&run), Some(StepProgress { done: 10, total: Some(10) }));
    }
}
