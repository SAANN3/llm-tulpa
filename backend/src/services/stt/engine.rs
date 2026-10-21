//! The speech recognizer itself: loading a model from its folder and reading audio with it. One model is loaded at a
//! time (they take a gigabyte or two), and reads go one at a time, so a second user waits instead of doubling the
//! load on the CPU. Everything here blocks: callers run it on a blocking thread.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sherpa_onnx::{
    OfflineOmnilingualAsrCtcModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
    OfflineWhisperModelConfig,
};

use super::catalog::{Family, ModelSpec};
use super::session::SAMPLE_RATE;

/// The start of a clip read once before the timed read: the first read of a loaded model is slower than the rest
const WARMUP_SAMPLES: usize = SAMPLE_RATE * 2;

/// The loudness the audio is brought to: the level that 95 % of its samples stay under. Speech recorded without the
/// browser's auto-gain can come in very quiet, and a model reads quiet speech as nothing or as the wrong words.
const TARGET_LEVEL: f32 = 0.2;
/// How much a quiet recording may be amplified, so that a room's hiss is not brought up to speech
const MAX_GAIN: f32 = 8.0;
/// Below this level there is nothing but silence to amplify
const SILENCE_LEVEL: f32 = 0.003;
/// Only every this-many'th sample is looked at to find the level: enough for a percentile, and cheap on 20 s of audio
const LEVEL_STRIDE: usize = 4;
/// The share of samples that stay under the level; a click or a pop at the start can't set the level the way the peak would
const LEVEL_PERCENTILE: f32 = 0.95;

/// `audio` brought up to `TARGET_LEVEL` when it is quieter than that (never turned down: a loud recording is fine as it
/// is). The gain is worked out from the audio read, so a read of a longer window can differ a little from the one
/// before it.
fn evened(audio: &[f32]) -> Cow<'_, [f32]> {
    let mut levels: Vec<f32> = audio.iter().step_by(LEVEL_STRIDE).map(|sample| sample.abs()).collect();
    if levels.is_empty() {
        return Cow::Borrowed(audio);
    }
    let at = ((levels.len() - 1) as f32 * LEVEL_PERCENTILE) as usize;
    let (_, level, _) = levels.select_nth_unstable_by(at, |a, b| a.total_cmp(b));
    let level = *level;
    if level < SILENCE_LEVEL {
        return Cow::Borrowed(audio);
    }
    let gain = (TARGET_LEVEL / level).min(MAX_GAIN);
    if gain <= 1.0 {
        return Cow::Borrowed(audio);
    }
    Cow::Owned(audio.iter().map(|sample| (sample * gain).clamp(-1.0, 1.0)).collect())
}

struct Loaded {
    id: &'static str,
    /// Only a Whisper model is built for a language; for the others it is empty
    language: String,
    /// A recognizer is built for a thread count, so a changed count loads it again
    threads: i32,
    recognizer: Arc<OfflineRecognizer>,
    last_used: Instant,
}

#[derive(Default)]
pub(super) struct Engine {
    loaded: Mutex<Option<Loaded>>,
    /// Held while a recognizer reads, so reads of every session take turns
    reading: Mutex<()>,
}

/// The one file in `dir` whose name satisfies `wanted`; the shortest name when several do
fn file_where(dir: &Path, wanted: impl Fn(&str) -> bool) -> Result<String, String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("could not read {}: {e}", dir.display()))?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| wanted(name))
        .collect();
    names.sort_by_key(|name| name.len());
    names
        .into_iter()
        .next()
        .map(|name| dir.join(name).display().to_string())
        .ok_or_else(|| format!("{} is missing a file the model needs; remove it and install it again", dir.display()))
}

fn config_of(spec: &ModelSpec, dir: &Path, language: &str, threads: i32) -> Result<OfflineRecognizerConfig, String> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = threads;
    config.model_config.tokens = Some(file_where(dir, |n| n.ends_with("tokens.txt"))?);
    match spec.family {
        Family::NemoTransducer => {
            config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(file_where(dir, |n| n.starts_with("encoder") && n.ends_with(".onnx"))?),
                decoder: Some(file_where(dir, |n| n.starts_with("decoder") && n.ends_with(".onnx"))?),
                joiner: Some(file_where(dir, |n| n.starts_with("joiner") && n.ends_with(".onnx"))?),
            };
            config.model_config.model_type = Some("nemo_transducer".into());
        }
        Family::OmnilingualCtc => {
            config.model_config.omnilingual = OfflineOmnilingualAsrCtcModelConfig { model: Some(file_where(dir, |n| n.starts_with("model") && n.ends_with(".onnx"))?) };
        }
        Family::Whisper => {
            config.model_config.whisper = OfflineWhisperModelConfig {
                encoder: Some(file_where(dir, |n| n.contains("encoder") && n.ends_with(".onnx"))?),
                decoder: Some(file_where(dir, |n| n.contains("decoder") && n.ends_with(".onnx"))?),
                language: Some(language.to_string()),
                task: Some("transcribe".into()),
                ..Default::default()
            };
        }
    }
    Ok(config)
}

/// What `audio` (16 kHz mono) says; the caller holds the turn
fn decode(recognizer: &OfflineRecognizer, audio: &[f32]) -> String {
    let stream = recognizer.create_stream();
    stream.accept_waveform(SAMPLE_RATE as i32, &evened(audio));
    recognizer.decode(&stream);
    stream.get_result().map(|result| result.text.trim().to_string()).unwrap_or_default()
}

impl Engine {
    /// The recognizer for `spec` (and `language`, for a model that takes one), loaded now if it isn't. Loading a model
    /// takes a couple of seconds and replaces whichever other model was loaded.
    pub(super) fn recognizer(&self, spec: &'static ModelSpec, dir: PathBuf, language: &str, threads: i32) -> Result<Arc<OfflineRecognizer>, String> {
        let language = if spec.family == Family::Whisper { language } else { "" };
        let mut loaded = self.loaded.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(have) = loaded.as_mut().filter(|have| have.id == spec.id && have.language == language && have.threads == threads) {
            have.last_used = Instant::now();
            return Ok(have.recognizer.clone());
        }
        // The old one goes first: both at once would take the memory of two
        *loaded = None;
        let config = config_of(spec, &dir, language, threads)?;
        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| format!("{} could not be loaded; remove it and install it again", spec.label))?;
        let recognizer = Arc::new(recognizer);
        *loaded = Some(Loaded { id: spec.id, language: language.to_string(), threads, recognizer: recognizer.clone(), last_used: Instant::now() });
        Ok(recognizer)
    }

    /// What `audio` (16 kHz mono) says
    pub(super) fn read(&self, recognizer: &OfflineRecognizer, audio: &[f32]) -> String {
        let _turn = self.reading.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        decode(recognizer, audio)
    }

    /// How long `audio` takes to read with a recognizer of its own built for `threads`: not the shared one, so what is
    /// loaded for voice inputs stays as it is. The model is loaded before the clock starts, and a first read of the
    /// start of the clip warms it up, so the time is that of a read as a voice input makes it. Blocking, for a long
    /// while: callers run it on a blocking thread.
    pub(super) fn time_read(&self, spec: &'static ModelSpec, dir: PathBuf, language: &str, threads: i32, audio: &[f32]) -> Result<Duration, String> {
        let language = if spec.family == Family::Whisper { language } else { "" };
        let config = config_of(spec, &dir, language, threads)?;
        let recognizer = OfflineRecognizer::create(&config).ok_or_else(|| format!("{} could not be loaded; remove it and install it again", spec.label))?;
        let _turn = self.reading.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        decode(&recognizer, &audio[..audio.len().min(WARMUP_SAMPLES)]);
        let started = Instant::now();
        decode(&recognizer, audio);
        Ok(started.elapsed())
    }

    /// Lets go of the loaded model, for a measurement that loads models of its own: two at once would take the memory of two
    pub(super) fn unload_all(&self) {
        *self.loaded.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    /// Unloads the model when nothing has used it for `idle`; true when it did
    pub(super) fn unload_if_idle(&self, idle: Duration) -> bool {
        let mut loaded = self.loaded.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if loaded.as_ref().is_some_and(|have| have.last_used.elapsed() >= idle) {
            *loaded = None;
            return true;
        }
        false
    }

    /// Unloads `id` if it is the one loaded (its files are about to go)
    pub(super) fn unload(&self, id: &str) {
        let mut loaded = self.loaded.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if loaded.as_ref().is_some_and(|have| have.id == id) {
            *loaded = None;
        }
    }

    pub(super) fn loaded_id(&self) -> Option<&'static str> {
        self.loaded.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|have| have.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(level: f32, samples: usize) -> Vec<f32> {
        (0..samples).map(|i| level * (i as f32 * 0.3).sin()).collect()
    }

    #[test]
    fn quiet_speech_is_brought_up_to_the_target() {
        let audio = tone(0.03, 16_000);
        let out = evened(&audio);
        let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.15 && peak <= 0.3, "peak {peak}");
    }

    #[test]
    fn loud_audio_and_silence_are_left_alone() {
        assert!(matches!(evened(&tone(0.6, 16_000)), Cow::Borrowed(_)));
        assert!(matches!(evened(&tone(0.001, 16_000)), Cow::Borrowed(_)));
        assert!(matches!(evened(&[]), Cow::Borrowed(_)));
    }

    #[test]
    fn the_gain_is_capped_and_a_click_does_not_set_it() {
        let quiet = tone(0.01, 16_000);
        let out = evened(&quiet);
        let peak = out.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak <= 0.01 * MAX_GAIN + 1e-4, "peak {peak}");
        let mut with_click = tone(0.03, 16_000);
        with_click[100] = 1.0;
        let out = evened(&with_click);
        assert!(out[8_000].abs() > 0.03 || out[8_001].abs() > 0.03, "the click kept the audio from being brought up");
    }
}
