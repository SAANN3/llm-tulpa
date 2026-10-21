//! A live voice input: the audio a page has sent so far, and the text it has come to. The page sends chunks while the
//! user speaks and each answer carries the text so far, so the user sees it appear.
//!
//! The models read a whole utterance at a time, so the text is made by reading the audio again as it grows (a read
//! of 10 seconds takes about 0.7 s on 4 threads, 25 seconds about 2 s). To keep that bounded, once the part not yet
//! settled is longer than `MAX_WINDOW` it is cut at its quietest moment and the part before the cut is read one last
//! time and kept as it is.
//!
//! This is the state and the decisions only; the service runs the reads, so everything here can be checked without a
//! model.

use std::sync::Mutex;
use std::time::Instant;

use super::catalog::ModelSpec;

pub(super) const SAMPLE_RATE: usize = 16_000;
/// Nothing is read before this much audio is there: too short to say anything
const FIRST_READ: usize = SAMPLE_RATE / 2;
/// The text is refreshed each time this much audio has come in since the last read
const READ_STEP: usize = SAMPLE_RATE * 6 / 10;
/// The part not yet settled is read again as it grows up to this long
const MAX_WINDOW: usize = SAMPLE_RATE * 20;
/// The cut is looked for in the last stretch of the window, this long, never in its first seconds
const CUT_SEARCH: usize = SAMPLE_RATE * 6;
/// The quietest moment is judged over windows of this length
const CUT_FRAME: usize = SAMPLE_RATE / 10;
/// A session is refused more audio than this: five minutes
pub(super) const MAX_SAMPLES: usize = SAMPLE_RATE * 60 * 5;
/// Audio that ends a session shorter than this isn't read
const MIN_FINAL: usize = SAMPLE_RATE / 5;

/// A live text with fewer words than this is not kept for the end: a lone "Yeah." or "Uh" is what a microphone's
/// opening click is read as, and the next read drops it
const MIN_WORDS_KEPT: usize = 2;

/// What to read, and what to do with the text
pub(super) struct Job {
    window: Vec<f32>,
    /// Where the window starts in the session's audio
    from: usize,
    /// The text of the window up to this sample is final; `None` when the whole window is read as it stands
    cut: Option<usize>,
    /// How much audio there was when the job was made
    upto: usize,
}

impl Job {
    /// The audio to read: up to the cut, or all of the window
    pub(super) fn audio(&self) -> &[f32] {
        match self.cut {
            Some(cut) => &self.window[..cut],
            None => &self.window,
        }
    }
}

struct State {
    samples: Vec<f32>,
    /// What has been read and settled, and how much of the audio it covers
    committed: String,
    committed_to: usize,
    /// The reading of the rest, as of `read_to`
    partial: String,
    read_to: usize,
    reading: bool,
    /// The last live text that had something to say: the answer when the final read comes back empty
    kept: String,
}

pub(super) struct Session {
    pub user_id: i64,
    pub model: &'static ModelSpec,
    /// The language the model is told to expect; empty for it to find out
    pub language: String,
    state: Mutex<State>,
    pub touched: Mutex<Instant>,
}

/// The text so far: what is settled and the reading of the rest
fn joined(committed: &str, partial: &str) -> String {
    match (committed.is_empty(), partial.is_empty()) {
        (true, _) => partial.to_string(),
        (_, true) => committed.to_string(),
        _ => format!("{committed} {partial}"),
    }
}

/// Where the quietest stretch of `window` is, within its last `CUT_SEARCH` samples (leaving a second at the end)
fn quietest(window: &[f32]) -> usize {
    let end = window.len().saturating_sub(SAMPLE_RATE);
    let start = window.len().saturating_sub(CUT_SEARCH).min(end);
    let mut best = (f32::MAX, end);
    let mut at = start;
    while at + CUT_FRAME <= end {
        let energy: f32 = window[at..at + CUT_FRAME].iter().map(|s| s * s).sum();
        if energy < best.0 {
            best = (energy, at + CUT_FRAME / 2);
        }
        at += CUT_FRAME / 2;
    }
    best.1
}

impl Session {
    pub(super) fn new(user_id: i64, model: &'static ModelSpec, language: String) -> Self {
        Self {
            user_id,
            model,
            language,
            state: Mutex::new(State { samples: Vec::new(), committed: String::new(), committed_to: 0, partial: String::new(), read_to: 0, reading: false, kept: String::new() }),
            touched: Mutex::new(Instant::now()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Takes in audio. `Err` when the session has had all it takes; otherwise the read to run now, if one is due (one
    /// at a time: a read in progress means the next waits for it).
    pub(super) fn push(&self, audio: &[f32]) -> Result<Option<Job>, ()> {
        *self.touched.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Instant::now();
        let mut st = self.state();
        if st.samples.len() + audio.len() > MAX_SAMPLES {
            return Err(());
        }
        st.samples.extend_from_slice(audio);
        if st.reading {
            return Ok(None);
        }
        let pending = st.samples.len() - st.committed_to;
        let fresh = st.samples.len() - st.read_to.max(st.committed_to);
        let first = st.partial.is_empty() && st.read_to <= st.committed_to;
        if pending < FIRST_READ || (!first && fresh < READ_STEP) {
            return Ok(None);
        }
        st.reading = true;
        let window = st.samples[st.committed_to..].to_vec();
        let cut = (window.len() > MAX_WINDOW).then(|| quietest(&window));
        Ok(Some(Job { from: st.committed_to, upto: st.samples.len(), cut, window }))
    }

    /// Takes the text a job's audio read as; returns the text so far
    pub(super) fn apply(&self, job: &Job, text: &str) -> String {
        let mut st = self.state();
        st.reading = false;
        let text = text.trim();
        match job.cut {
            Some(cut) => {
                // The part before the cut is settled; what comes after is read afresh at the next step
                st.committed = joined(&st.committed, text);
                st.committed_to = job.from + cut;
                st.partial.clear();
                st.read_to = st.committed_to;
            }
            None => {
                st.partial = text.to_string();
                st.read_to = job.upto;
            }
        }
        let text = joined(&st.committed, &st.partial);
        if text.split_whitespace().count() >= MIN_WORDS_KEPT {
            st.kept = text.clone();
        }
        text
    }

    /// A read that failed: the session goes on with the text it has
    pub(super) fn failed(&self) {
        self.state().reading = false;
    }

    /// The text so far, without reading anything
    pub(super) fn text(&self) -> String {
        let st = self.state();
        joined(&st.committed, &st.partial)
    }

    /// The last live text with something to say; empty when there was none
    pub(super) fn kept(&self) -> String {
        self.state().kept.clone()
    }

    pub(super) fn is_reading(&self) -> bool {
        self.state().reading
    }

    /// What is left to read when the user is done: the audio not yet settled (`None` when there is too little), and
    /// what is settled
    pub(super) fn rest(&self) -> (Option<Vec<f32>>, String) {
        let st = self.state();
        let rest = &st.samples[st.committed_to..];
        ((rest.len() >= MIN_FINAL).then(|| rest.to_vec()), st.committed.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::stt::catalog::MODELS;

    fn session() -> Session {
        Session::new(1, &MODELS[0], String::new())
    }

    fn secs(n: f32, level: f32) -> Vec<f32> {
        vec![level; (n * SAMPLE_RATE as f32) as usize]
    }

    #[test]
    fn nothing_is_read_before_there_is_something_to_say() {
        let s = session();
        assert!(s.push(&secs(0.3, 0.1)).unwrap().is_none());
        let job = s.push(&secs(0.3, 0.1)).unwrap().expect("0.6 s is enough for the first read");
        assert_eq!(job.audio().len(), (0.6 * SAMPLE_RATE as f32) as usize);
    }

    #[test]
    fn reads_are_one_at_a_time_and_follow_the_growth() {
        let s = session();
        let first = s.push(&secs(1.0, 0.1)).unwrap().unwrap();
        // A read is running: the audio that comes meanwhile waits for it
        assert!(s.push(&secs(1.0, 0.1)).unwrap().is_none());
        assert!(s.is_reading());
        assert_eq!(s.apply(&first, "hello"), "hello");
        // The audio that waited is more than a step: the next push brings a read of all of it
        let second = s.push(&secs(0.2, 0.1)).unwrap().expect("a step of audio since the last read");
        assert_eq!(second.audio().len(), (2.2 * SAMPLE_RATE as f32) as usize);
        assert_eq!(s.apply(&second, "hello there"), "hello there");
        // Less than a step of new audio: no read yet
        assert!(s.push(&secs(0.2, 0.1)).unwrap().is_none());
        let third = s.push(&secs(0.5, 0.1)).unwrap().expect("a step of audio since the last read");
        assert_eq!(third.audio().len(), (2.9 * SAMPLE_RATE as f32) as usize);
        assert_eq!(s.apply(&third, "hello there you"), "hello there you");
    }

    #[test]
    fn a_long_utterance_is_cut_at_its_quietest_moment_and_what_is_before_the_cut_is_kept() {
        let s = session();
        // 21 s of speech-like noise with a pause at 16 s
        let mut audio = secs(21.0, 0.2);
        for sample in audio.iter_mut().skip(16 * SAMPLE_RATE).take(SAMPLE_RATE / 2) {
            *sample = 0.0;
        }
        let job = s.push(&audio).unwrap().unwrap();
        let cut = job.cut.expect("longer than the window");
        let pause = 16 * SAMPLE_RATE..16 * SAMPLE_RATE + SAMPLE_RATE / 2;
        assert!(pause.contains(&cut), "the cut {cut} is in the pause");
        assert_eq!(job.audio().len(), cut);
        assert_eq!(s.apply(&job, "the first part"), "the first part");
        // The rest is read afresh; what was settled stays in front of it
        let next = s.push(&secs(1.0, 0.2)).unwrap().expect("the rest is pending");
        assert!(next.cut.is_none());
        assert_eq!(next.from, cut);
        assert_eq!(s.apply(&next, "and the end"), "the first part and the end");
    }

    #[test]
    fn what_is_left_when_the_user_is_done_is_the_unsettled_audio() {
        let s = session();
        let job = s.push(&secs(2.0, 0.1)).unwrap().unwrap();
        s.apply(&job, "so far");
        let (rest, committed) = s.rest();
        assert_eq!(rest.map(|r| r.len()), Some(2 * SAMPLE_RATE));
        assert_eq!(committed, "");
        let tiny = session();
        tiny.push(&secs(0.1, 0.1)).unwrap();
        assert!(tiny.rest().0.is_none(), "too little to read");
    }

    #[test]
    fn a_session_refuses_more_than_five_minutes() {
        let s = session();
        s.push(&secs(299.0, 0.0)).ok();
        assert!(s.push(&secs(2.0, 0.0)).is_err());
    }

    #[test]
    fn the_last_live_text_with_words_is_kept_and_a_lone_word_is_not() {
        let s = session();
        let job = s.push(&secs(0.6, 0.1)).unwrap().unwrap();
        s.apply(&job, "Yeah.");
        assert_eq!(s.kept(), "");
        let job = s.push(&secs(0.7, 0.1)).unwrap().unwrap();
        s.apply(&job, "The quick brown fox");
        let job = s.push(&secs(0.7, 0.1)).unwrap().unwrap();
        s.apply(&job, "");
        assert_eq!(s.kept(), "The quick brown fox");
    }
}
